use std::{
    fs,
    io::{self, Read, Write},
    sync::mpsc,
    thread,
};

#[cfg(target_os = "windows")]
use std::{
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    sync::Arc,
};

// Unix desktop responses are newline-framed; keep one reader for the session.
// Windows reads whole messages from a message-mode pipe instead.
#[cfg(unix)]
use std::io::{BufRead, BufReader};

use banana_hand_protocol::{
    HostBridgeRequest, HostBridgeResponse, NativeHostBridgeConfig, NativeHostMessage,
    PROTOCOL_MAJOR, local_runtime_directory,
};
use serde_json::{Value, json};
use thiserror::Error;

const MAX_NATIVE_MESSAGE_BYTES: usize = 1_048_576;
/// Message-mode pipe buffers are sized to hold the largest single message in
/// one Read/Write, so framing stays one-object-per-call.
#[cfg(target_os = "windows")]
const PIPE_BUFFER_BYTES: usize = 2 * MAX_NATIVE_MESSAGE_BYTES;

#[derive(Debug, Error)]
enum HostError {
    #[error("native messaging frame exceeds {MAX_NATIVE_MESSAGE_BYTES} bytes")]
    FrameTooLarge,
    #[error("native messaging frame was not valid JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("bridge configuration unavailable: {0}")]
    BridgeConfig(#[source] io::Error),
    #[error("bridge is unavailable: {0}")]
    BridgeUnavailable(#[source] io::Error),
}

enum RelayEvent {
    Desktop(Result<Value, HostError>),
    Browser(Option<Value>),
}

/// The native host is a short-lived, browser-launched process. On any hard
/// failure (missing/invalid config, unreachable desktop bridge, framing error)
/// it must exit non-zero so the browser's native-messaging layer treats the
/// launch as failed and never mistakes it for a successful no-op.
fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--self-check") {
        match self_check() {
            Ok(message) => {
                println!("{message}");
                return;
            }
            Err(error) => {
                eprintln!("banana-hand-native-host: {error}");
                std::process::exit(1);
            }
        }
    }
    if let Err(error) = run() {
        eprintln!("banana-hand-native-host: {error}");
        std::process::exit(1);
    }
}

/// Verify the desktop half of the bridge without native-messaging framing:
/// the config file must be readable and the desktop socket (or named pipe)
/// must accept a connection. The app runs this on startup to surface
/// failures the browser would hide — most importantly a host binary the
/// operating system refuses to run (macOS Gatekeeper quarantine), where
/// `connectNative` can only ever say "Native host has exited".
fn self_check() -> Result<String, HostError> {
    let config = read_bridge_config()?;
    let mut transport = DesktopTransport::connect(&config)?;

    // Exercise the full round trip, not just the pipe/socket connection: send
    // a hello and require an ack, so a framing or parsing regression fails the
    // check instead of surfacing later as a rejected browser handshake.
    let hello = json!({
        "type": "hello",
        "request_id": "self-check",
        "protocol_major": PROTOCOL_MAJOR,
        "browser": "firefox",
        "browser_instance_id": "self-check",
        "session_nonce": "self-check",
    });
    let request = HostBridgeRequest {
        capability_token: config.capability_token.clone(),
        message: NativeHostMessage {
            request_id: "self-check".into(),
            message: hello,
        },
    };
    let mut framed = serde_json::to_vec(&request)?;
    framed.push(b'\n');
    transport.write_bytes(&framed)?;
    let response = transport.read_response()?;
    if response.get("type").and_then(Value::as_str) != Some("ack") {
        return Err(HostError::BridgeUnavailable(io::Error::other(format!(
            "desktop answered self-check with an unexpected response: {response}"
        ))));
    }
    Ok("self-check ok: desktop bridge reachable".into())
}

/// The desktop-side bridge connection. Exactly one variant exists per target
/// platform: a Unix domain socket (Linux/macOS) or a named pipe (Windows).
/// Both carry the same newline-framed `HostBridgeRequest`/`HostBridgeResponse`
/// JSON, so the dispatch loop in `run()` is transport-agnostic.
enum DesktopTransport {
    #[cfg(unix)]
    Unix(std::os::unix::net::UnixStream),
    #[cfg(target_os = "windows")]
    Pipe(Arc<OwnedHandle>),
}

impl Clone for DesktopTransport {
    fn clone(&self) -> Self {
        match self {
            #[cfg(unix)]
            Self::Unix(stream) => Self::Unix(stream.try_clone().expect("clone socket")),
            #[cfg(target_os = "windows")]
            Self::Pipe(handle) => Self::Pipe(Arc::clone(handle)),
            #[cfg(not(any(unix, windows)))]
            _ => unreachable!(),
        }
    }
}

impl DesktopTransport {
    fn connect(config: &NativeHostBridgeConfig) -> Result<Self, HostError> {
        #[cfg(unix)]
        {
            let path = config.socket_path.as_ref().ok_or_else(|| {
                HostError::BridgeConfig(io::Error::other("bridge config has no unix socket path"))
            })?;
            let stream = std::os::unix::net::UnixStream::connect(path)
                .map_err(HostError::BridgeUnavailable)?;
            Ok(Self::Unix(stream))
        }
        #[cfg(target_os = "windows")]
        {
            let name = config.pipe_name.as_deref().ok_or_else(|| {
                HostError::BridgeConfig(io::Error::other("bridge config has no named-pipe name"))
            })?;
            let handle = open_named_pipe(name)?;
            Ok(Self::Pipe(Arc::new(handle)))
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = config;
            Err(HostError::BridgeUnavailable(io::Error::new(
                io::ErrorKind::Unsupported,
                "this host build has no desktop bridge transport",
            )))
        }
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), HostError> {
        #[cfg(unix)]
        {
            let stream = match self {
                Self::Unix(stream) => stream,
            };
            stream
                .write_all(bytes)
                .map_err(HostError::BridgeUnavailable)?;
            stream.flush().map_err(HostError::BridgeUnavailable)
        }
        #[cfg(target_os = "windows")]
        {
            let handle = match self {
                Self::Pipe(handle) => handle,
            };
            pipe_write(handle, bytes)
        }
    }

    /// Read one newline-framed response line (Unix) or one message-mode
    /// message (Windows) and return the inner response value the desktop
    /// dispatched (matching `read_desktop_messages`).
    fn read_response(&mut self) -> Result<Value, HostError> {
        #[cfg(unix)]
        {
            let stream = match self {
                Self::Unix(stream) => stream,
            };
            let mut line = String::new();
            BufReader::new(stream)
                .read_line(&mut line)
                .map_err(HostError::BridgeUnavailable)?;
            let response: HostBridgeResponse =
                serde_json::from_str(line.trim_end()).map_err(HostError::InvalidJson)?;
            Ok(response.response)
        }
        #[cfg(target_os = "windows")]
        {
            let handle = match self {
                Self::Pipe(handle) => handle,
            };
            let line = pipe_read_message(handle)?.ok_or_else(|| {
                HostError::BridgeUnavailable(io::Error::other("desktop closed without a response"))
            })?;
            let response: HostBridgeResponse =
                serde_json::from_str(&line).map_err(HostError::InvalidJson)?;
            Ok(response.response)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = self;
            Err(HostError::BridgeUnavailable(io::Error::new(
                io::ErrorKind::Unsupported,
                "this host build has no desktop bridge transport",
            )))
        }
    }
}

#[cfg(target_os = "windows")]
fn open_named_pipe(name: &str) -> Result<OwnedHandle, HostError> {
    use windows_sys::Win32::{
        Foundation::{GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{
            CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_OVERLAPPED, OPEN_EXISTING,
        },
        System::Pipes::{PIPE_READMODE_MESSAGE, SetNamedPipeHandleState},
    };
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            (GENERIC_READ | GENERIC_WRITE) as u32,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OVERLAPPED,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(HostError::BridgeUnavailable(io::Error::last_os_error()));
    }
    let handle = unsafe { OwnedHandle::from_raw_handle(handle) };
    // The desktop writes one JSON message per WriteFile; read exactly one
    // message per ReadFile so a byte-mode read can never split or merge them.
    let mode = PIPE_READMODE_MESSAGE;
    let ok = unsafe {
        SetNamedPipeHandleState(
            handle.as_raw_handle(),
            &mode,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if ok == 0 {
        return Err(HostError::BridgeUnavailable(io::Error::last_os_error()));
    }
    Ok(handle)
}

#[cfg(target_os = "windows")]
/// Issue one overlapped I/O and wait inline for its completion. The reader and
/// writer threads share one handle, so I/O must be overlapped: synchronous
/// ReadFile and WriteFile on the same handle serialize, deadlocking the moment
/// both the read and the write are pending. The borrowed pipe, operation buffer,
/// OVERLAPPED, and owned event stay alive until the operation completes.
fn overlapped_io(
    handle: &OwnedHandle,
    issue: impl FnOnce(&mut windows_sys::Win32::System::IO::OVERLAPPED) -> i32,
) -> io::Result<u32> {
    use windows_sys::Win32::{
        Foundation::ERROR_IO_PENDING,
        System::IO::{GetOverlappedResult, OVERLAPPED},
        System::Threading::CreateEventW,
    };
    let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
    if event.is_null() {
        return Err(io::Error::last_os_error());
    }
    let event = unsafe { OwnedHandle::from_raw_handle(event) };
    let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
    overlapped.hEvent = event.as_raw_handle();

    if issue(&mut overlapped) == 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(ERROR_IO_PENDING as i32) {
            return Err(error);
        }
    }

    let mut transferred = 0_u32;
    let ok = unsafe {
        GetOverlappedResult(handle.as_raw_handle(), &mut overlapped, &mut transferred, 1)
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(transferred)
}

#[cfg(target_os = "windows")]
fn pipe_write(handle: &OwnedHandle, bytes: &[u8]) -> Result<(), HostError> {
    use windows_sys::Win32::Storage::FileSystem::WriteFile;
    let length = u32::try_from(bytes.len()).map_err(|_| {
        HostError::BridgeUnavailable(io::Error::new(
            io::ErrorKind::InvalidInput,
            "named pipe message is too large",
        ))
    })?;
    let written = overlapped_io(handle, |overlapped| unsafe {
        WriteFile(
            handle.as_raw_handle(),
            bytes.as_ptr(),
            length,
            std::ptr::null_mut(),
            overlapped,
        )
    })
    .map_err(HostError::BridgeUnavailable)?;
    if written != length {
        return Err(HostError::BridgeUnavailable(io::Error::new(
            io::ErrorKind::WriteZero,
            "named pipe write did not send the complete message",
        )));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn pipe_read_message(handle: &OwnedHandle) -> Result<Option<String>, HostError> {
    use windows_sys::Win32::{
        Foundation::{ERROR_BROKEN_PIPE, ERROR_NO_DATA, ERROR_PIPE_NOT_CONNECTED},
        Storage::FileSystem::ReadFile,
    };
    let mut buffer = vec![0_u8; PIPE_BUFFER_BYTES];
    let read = match overlapped_io(handle, |overlapped| unsafe {
        ReadFile(
            handle.as_raw_handle(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            std::ptr::null_mut(),
            overlapped,
        )
    }) {
        Ok(read) => read,
        Err(error)
            if matches!(
                error.raw_os_error().map(|code| code as u32),
                Some(ERROR_BROKEN_PIPE | ERROR_NO_DATA | ERROR_PIPE_NOT_CONNECTED)
            ) =>
        {
            return Ok(None);
        }
        Err(error) => return Err(HostError::BridgeUnavailable(error)),
    };
    if read == 0 {
        return Ok(None);
    }
    let message = String::from_utf8_lossy(&buffer[..read as usize]);
    Ok(Some(message.trim_end_matches('\n').to_owned()))
}

fn run() -> Result<(), HostError> {
    let config = read_bridge_config()?;
    let mut bridge_writer = DesktopTransport::connect(&config)?;
    let transport = bridge_writer.clone();

    let (sender, receiver) = mpsc::channel();
    let desktop_sender = sender.clone();
    thread::spawn(move || {
        if let Err(error) = read_desktop_messages(transport, &desktop_sender) {
            let _ = desktop_sender.send(RelayEvent::Desktop(Err(error)));
        }
    });
    thread::spawn(move || read_browser_messages(sender));

    let stdout = io::stdout();
    let mut output = stdout.lock();
    for event in receiver {
        match event {
            RelayEvent::Desktop(message) => write_native_message(&mut output, &message?)?,
            RelayEvent::Browser(Some(message)) => {
                let request_id = message
                    .get("request_id")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_owned();
                let request = HostBridgeRequest {
                    capability_token: config.capability_token.clone(),
                    message: NativeHostMessage {
                        request_id,
                        message,
                    },
                };
                let encoded = serde_json::to_vec(&request)?;
                // Emit the JSON and its newline in a single write so the frame
                // is one message on Windows message-mode pipes and stays one
                // newline-terminated line on Unix streams.
                let mut framed = encoded;
                framed.push(b'\n');
                bridge_writer.write_bytes(&framed)?;
            }
            RelayEvent::Browser(None) => break,
        }
    }
    Ok(())
}

fn read_bridge_config() -> Result<NativeHostBridgeConfig, HostError> {
    let runtime_directory = local_runtime_directory().ok_or_else(|| {
        HostError::BridgeConfig(io::Error::other(
            "no native host runtime directory available on this platform",
        ))
    })?;
    let raw = fs::read_to_string(runtime_directory.join("bridge.json"))
        .map_err(HostError::BridgeConfig)?;
    serde_json::from_str(&raw).map_err(HostError::InvalidJson)
}

fn read_desktop_messages(
    transport: DesktopTransport,
    sender: &mpsc::Sender<RelayEvent>,
) -> Result<(), HostError> {
    #[cfg(unix)]
    let mut reader = match transport {
        DesktopTransport::Unix(stream) => BufReader::new(stream),
    };
    #[cfg(target_os = "windows")]
    let handle = match transport {
        DesktopTransport::Pipe(handle) => handle,
    };
    let mut line = String::new();
    loop {
        line.clear();
        #[cfg(unix)]
        let length = reader
            .read_line(&mut line)
            .map_err(HostError::BridgeUnavailable)?;
        #[cfg(target_os = "windows")]
        let length = match pipe_read_message(&handle)? {
            Some(message) => {
                line = message;
                line.len()
            }
            None => 0,
        };
        if length == 0 {
            return Err(HostError::BridgeUnavailable(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "桌面 App 已中斷橋接連線",
            )));
        }
        if let Ok(response) = serde_json::from_str::<HostBridgeResponse>(&line) {
            if sender
                .send(RelayEvent::Desktop(Ok(response.response)))
                .is_err()
            {
                return Ok(());
            }
        }
    }
}

fn read_browser_messages(sender: mpsc::Sender<RelayEvent>) {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    loop {
        match read_native_message(&mut input) {
            Ok(Some(message)) => {
                if sender.send(RelayEvent::Browser(Some(message))).is_err() {
                    return;
                }
            }
            Ok(None) | Err(_) => {
                let _ = sender.send(RelayEvent::Browser(None));
                return;
            }
        }
    }
}

fn read_native_message(reader: &mut impl Read) -> Result<Option<Value>, HostError> {
    let mut length = [0_u8; 4];
    match reader.read_exact(&mut length) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(HostError::BridgeUnavailable(error)),
    }
    let length = u32::from_le_bytes(length) as usize;
    if length > MAX_NATIVE_MESSAGE_BYTES {
        return Err(HostError::FrameTooLarge);
    }
    let mut body = vec![0; length];
    reader
        .read_exact(&mut body)
        .map_err(HostError::BridgeUnavailable)?;
    Ok(Some(serde_json::from_slice(&body)?))
}

fn write_native_message(writer: &mut impl Write, message: &Value) -> Result<(), HostError> {
    let body = serde_json::to_vec(message)?;
    if body.len() > MAX_NATIVE_MESSAGE_BYTES {
        return Err(HostError::FrameTooLarge);
    }
    writer
        .write_all(&(body.len() as u32).to_le_bytes())
        .map_err(HostError::BridgeUnavailable)?;
    writer
        .write_all(&body)
        .map_err(HostError::BridgeUnavailable)?;
    writer.flush().map_err(HostError::BridgeUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Cursor;

    #[test]
    fn native_message_round_trip_preserves_json() {
        let original = json!({"type": "hello", "request_id": "request-1"});
        let mut bytes = Vec::new();
        write_native_message(&mut bytes, &original).unwrap();
        assert_eq!(
            read_native_message(&mut Cursor::new(bytes)).unwrap(),
            Some(original)
        );
    }

    #[test]
    fn rejects_oversized_native_message() {
        let mut bytes = (MAX_NATIVE_MESSAGE_BYTES as u32 + 1).to_le_bytes().to_vec();
        bytes.extend_from_slice(b"{}");
        assert!(matches!(
            read_native_message(&mut Cursor::new(bytes)),
            Err(HostError::FrameTooLarge)
        ));
    }

    #[test]
    fn bridge_config_without_transport_fails_closed() {
        // A config that names neither a socket nor a pipe must fail closed,
        // never guess a transport.
        let config = NativeHostBridgeConfig {
            socket_path: None,
            pipe_name: None,
            capability_token: "token".into(),
        };
        let result = DesktopTransport::connect(&config);
        assert!(result.is_err());
    }
}
