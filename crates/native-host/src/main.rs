use std::{
    fs,
    io::{self, Read, Write},
    sync::{Arc, mpsc},
    thread,
    time::Duration,
};

// `BufReader`/`BufRead` are only used by the Unix branch of `read_line`;
// the Windows branch reads whole messages from a message-mode pipe instead.
#[cfg(unix)]
use std::io::{BufRead, BufReader};

use banana_hand_protocol::{
    HostBridgeRequest, HostBridgeResponse, NativeHostBridgeConfig, NativeHostMessage,
    local_runtime_directory,
};
use serde_json::Value;
use thiserror::Error;

use parking_lot::Mutex;

const MAX_NATIVE_MESSAGE_BYTES: usize = 1_048_576;
#[cfg(target_os = "windows")]
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

/// The native host is a short-lived, browser-launched process. On any hard
/// failure (missing/invalid config, unreachable desktop bridge, framing error)
/// it must exit non-zero so the browser's native-messaging layer treats the
/// launch as failed and never mistakes it for a successful no-op.
fn main() {
    if let Err(error) = run() {
        eprintln!("banana-hand-native-host: {error}");
        std::process::exit(1);
    }
}

/// The desktop-side bridge connection. Exactly one variant exists per target
/// platform: a Unix domain socket (Linux/macOS) or a named pipe (Windows).
/// Both carry the same newline-framed `HostBridgeRequest`/`HostBridgeResponse`
/// JSON, so the dispatch loop in `run()` is transport-agnostic.
enum DesktopTransport {
    #[cfg(unix)]
    Unix(std::os::unix::net::UnixStream),
    #[cfg(target_os = "windows")]
    #[allow(dead_code)]
    Pipe(windows_sys::Win32::Foundation::HANDLE),
}

impl Clone for DesktopTransport {
    fn clone(&self) -> Self {
        match self {
            #[cfg(unix)]
            Self::Unix(stream) => Self::Unix(stream.try_clone().expect("clone socket")),
            #[cfg(target_os = "windows")]
            Self::Pipe(handle) => Self::Pipe(*handle),
            #[cfg(not(any(unix, windows)))]
            _ => unreachable!(),
        }
    }
}

// A Windows named pipe `HANDLE` is a process-wide value (not thread-local), so
// moving the handle between threads is safe. Unix sockets are already Send, so
// the impl is only needed on Windows.
#[cfg(target_os = "windows")]
unsafe impl Send for DesktopTransport {}

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
            Ok(Self::Pipe(handle))
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
                Self::Pipe(handle) => *handle,
            };
            pipe_write(handle, bytes)
        }
    }

    /// Reads one newline-framed object. On a message-mode pipe a single
    /// Read/Write round trip is exactly one such object.
    fn read_line(&mut self) -> Result<Option<String>, HostError> {
        #[cfg(unix)]
        {
            let stream = match self {
                Self::Unix(stream) => stream,
            };
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => Ok(None),
                Ok(_) => Ok(Some(line)),
                Err(error) => Err(HostError::BridgeUnavailable(error)),
            }
        }
        #[cfg(target_os = "windows")]
        {
            let handle = match self {
                Self::Pipe(handle) => *handle,
            };
            pipe_read_message(handle)
        }
    }
}

#[cfg(target_os = "windows")]
fn open_named_pipe(name: &str) -> Result<windows_sys::Win32::Foundation::HANDLE, HostError> {
    use windows_sys::Win32::{
        Foundation::{GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{CreateFileW, FILE_ATTRIBUTE_NORMAL, OPEN_EXISTING},
    };
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            (GENERIC_READ | GENERIC_WRITE) as u32,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(HostError::BridgeUnavailable(io::Error::other(
            "named pipe connect failed",
        )));
    }
    Ok(handle)
}

#[cfg(target_os = "windows")]
fn pipe_write(
    handle: windows_sys::Win32::Foundation::HANDLE,
    bytes: &[u8],
) -> Result<(), HostError> {
    use windows_sys::Win32::Storage::FileSystem::WriteFile;
    let mut written = 0_u32;
    let ok = unsafe {
        WriteFile(
            handle,
            bytes.as_ptr(),
            bytes.len() as u32,
            &mut written,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(HostError::BridgeUnavailable(io::Error::other(
            "named pipe write failed",
        )));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn pipe_read_message(
    handle: windows_sys::Win32::Foundation::HANDLE,
) -> Result<Option<String>, HostError> {
    use windows_sys::Win32::{Foundation::CloseHandle, Storage::FileSystem::ReadFile};
    let mut buffer = vec![0_u8; PIPE_BUFFER_BYTES];
    let mut read = 0_u32;
    let ok = unsafe {
        ReadFile(
            handle,
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            &mut read,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 || read == 0 {
        unsafe {
            CloseHandle(handle);
        }
        return Ok(None);
    }
    let message = String::from_utf8_lossy(&buffer[..read as usize]);
    Ok(Some(message.trim_end_matches('\n').to_owned()))
}

fn run() -> Result<(), HostError> {
    let config = read_bridge_config()?;
    let transport = DesktopTransport::connect(&config)?;
    let bridge_writer = Arc::new(Mutex::new(transport.clone()));

    let (desktop_tx, desktop_rx) = mpsc::channel();
    thread::spawn(move || read_desktop_messages(transport, desktop_tx));

    let (browser_tx, browser_rx) = mpsc::channel();
    thread::spawn(move || read_browser_messages(browser_tx));

    let stdout = io::stdout();
    let mut output = stdout.lock();
    loop {
        while let Ok(message) = desktop_rx.try_recv() {
            write_native_message(&mut output, &message)?;
        }

        match browser_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Some(message)) => {
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
                let mut writer = bridge_writer.lock();
                writer.write_bytes(&encoded)?;
                writer.write_bytes(b"\n")?;
            }
            Ok(None) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
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

fn read_desktop_messages(transport: DesktopTransport, sender: mpsc::Sender<Value>) {
    let mut transport = transport;
    loop {
        match transport.read_line() {
            Ok(Some(line)) => match serde_json::from_str::<HostBridgeResponse>(&line) {
                Ok(response) => {
                    if sender.send(response.response).is_err() {
                        return;
                    }
                }
                Err(_) => {}
            },
            Ok(None) | Err(_) => return,
        }
    }
}

fn read_browser_messages(sender: mpsc::Sender<Option<Value>>) {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    loop {
        match read_native_message(&mut input) {
            Ok(Some(message)) => {
                if sender.send(Some(message)).is_err() {
                    return;
                }
            }
            Ok(None) | Err(_) => {
                let _ = sender.send(None);
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
