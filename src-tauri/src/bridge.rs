use std::{
    fs,
    sync::{Arc, mpsc::Sender},
    thread,
};

#[cfg(not(target_os = "windows"))]
use std::io::{BufRead, BufReader, Write};

use banana_hand_protocol::{
    BrowserKind, HostBridgeRequest, HostBridgeResponse, NativeHostBridgeConfig, PROTOCOL_MAJOR,
    TabMetadata, TabTarget, local_runtime_directory,
};
use serde::Deserialize;
use serde_json::{Value, json};

use parking_lot::Mutex;

use crate::{DispatchCoordinator, target_key};

#[derive(Debug, Deserialize)]
struct Hello {
    #[serde(rename = "type")]
    message_type: String,
    protocol_major: u16,
    browser: BrowserKind,
    browser_instance_id: String,
    session_nonce: String,
    /// The browser's own diagnosis of the previous disconnect (e.g.
    /// "Native messaging host not found"), reported so the App can show it
    /// when no host is connected. Older extensions omit the field.
    #[serde(default)]
    last_disconnect_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TabsSnapshot {
    #[serde(rename = "type")]
    message_type: String,
    browser_instance_id: String,
    session_nonce: String,
    tabs: Vec<TabMetadata>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct PreparedResult {
    #[serde(rename = "type")]
    message_type: String,
    pub(crate) request_id: String,
    pub(crate) ready: bool,
    pub(crate) code: Option<String>,
    pub(crate) detail: Option<String>,
}

#[cfg(unix)]
pub fn start(coordinator: Arc<Mutex<DispatchCoordinator>>) -> Result<(), String> {
    use std::os::unix::{fs::PermissionsExt, net::UnixListener};

    let runtime_directory = local_runtime_directory()
        .ok_or("no native host runtime directory available on this platform")?;
    fs::create_dir_all(&runtime_directory).map_err(|error| error.to_string())?;
    fs::set_permissions(&runtime_directory, fs::Permissions::from_mode(0o700))
        .map_err(|error| error.to_string())?;

    let socket_path = runtime_directory.join("bridge.sock");
    if socket_path.exists() {
        fs::remove_file(&socket_path).map_err(|error| error.to_string())?;
    }
    let listener = UnixListener::bind(&socket_path).map_err(|error| error.to_string())?;
    fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))
        .map_err(|error| error.to_string())?;

    let capability_token = generate_capability_token();
    let config = NativeHostBridgeConfig {
        socket_path: Some(socket_path),
        pipe_name: None,
        capability_token: capability_token.clone(),
    };
    write_bridge_config(&runtime_directory, &config)?;

    thread::Builder::new()
        .name("banana-hand-native-bridge".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let coordinator = coordinator.clone();
                let capability_token = capability_token.clone();
                thread::spawn(move || serve_host(stream, coordinator, capability_token));
            }
        })
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn start(coordinator: Arc<Mutex<DispatchCoordinator>>) -> Result<(), String> {
    let runtime_directory = local_runtime_directory()
        .ok_or("no native host runtime directory available on this platform")?;
    fs::create_dir_all(&runtime_directory).map_err(|error| error.to_string())?;

    // One named pipe per app process; the exact name is published in the
    // config so the browser-launched host can connect to this instance.
    let pipe_name = format!(r"\\.\pipe\banana-hand-{}", std::process::id());
    let capability_token = generate_capability_token();
    let config = NativeHostBridgeConfig {
        socket_path: None,
        pipe_name: Some(pipe_name.clone()),
        capability_token: capability_token.clone(),
    };
    write_bridge_config(&runtime_directory, &config)?;

    let pipe_wide: Vec<u16> = pipe_name.encode_utf16().chain(std::iter::once(0)).collect();
    thread::Builder::new()
        .name("banana-hand-native-bridge".into())
        .spawn(move || accept_named_pipes(coordinator, pipe_wide, capability_token))
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(not(any(unix, windows)))]
pub fn start(_coordinator: Arc<Mutex<DispatchCoordinator>>) -> Result<(), String> {
    Ok(())
}

fn generate_capability_token() -> String {
    rand::random::<[u8; 32]>()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn write_bridge_config(
    runtime_directory: &std::path::Path,
    config: &NativeHostBridgeConfig,
) -> Result<(), String> {
    let config_path = runtime_directory.join("bridge.json");
    fs::write(
        &config_path,
        serde_json::to_vec(config).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(unix)]
fn serve_host(
    stream: std::os::unix::net::UnixStream,
    coordinator: Arc<Mutex<DispatchCoordinator>>,
    capability_token: String,
) {
    let (writer, receiver) = std::sync::mpsc::channel::<Value>();
    let Ok(mut output) = stream.try_clone() else {
        return;
    };
    thread::spawn(move || {
        for response in receiver {
            let Ok(encoded) = serde_json::to_string(&HostBridgeResponse { response }) else {
                continue;
            };
            if output.write_all(encoded.as_bytes()).is_err()
                || output.write_all(b"\n").is_err()
                || output.flush().is_err()
            {
                return;
            }
        }
    });

    let mut input = BufReader::new(stream);
    let mut registered_key: Option<String> = None;
    loop {
        let mut line = String::new();
        match input.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let (response, connection_key) =
                    dispatch_inbound(&coordinator, &line, &capability_token, writer.clone());
                if let Some(key) = connection_key {
                    registered_key = Some(key);
                }
                if writer.send(response).is_err() {
                    break;
                }
            }
        }
    }
    // The browser session is gone: drop its port registration and the tabs it
    // reported so the App never offers stale targets.
    if let Some(key) = &registered_key {
        let mut coordinator = coordinator.lock();
        coordinator.browser_ports.remove(key);
        let prefix = format!("{key}:");
        coordinator
            .connected_tabs
            .retain(|target_key, _| !target_key.starts_with(&prefix));
    }
}

#[cfg(target_os = "windows")]
fn accept_named_pipes(
    coordinator: Arc<Mutex<DispatchCoordinator>>,
    pipe_wide: Vec<u16>,
    capability_token: String,
) {
    // FILE_FLAG_FIRST_PIPE_INSTANCE is only valid on the very first
    // CreateNamedPipe for a name; after that it must be cleared or creation
    // fails. Track whether we've started the name yet.
    let mut first_instance = true;
    loop {
        let Some(pipe) = create_named_pipe(&pipe_wide, first_instance) else {
            thread::sleep(std::time::Duration::from_millis(100));
            continue;
        };
        first_instance = false;
        if connect_named_pipe(pipe) {
            let coordinator = coordinator.clone();
            let capability_token = capability_token.clone();
            thread::spawn(move || serve_pipe(pipe, coordinator, capability_token));
        } else {
            close_pipe(pipe);
        }
    }
}

#[cfg(target_os = "windows")]
fn serve_pipe(
    pipe: named_pipe::PipeHandle,
    coordinator: Arc<Mutex<DispatchCoordinator>>,
    capability_token: String,
) {
    let (writer, receiver) = std::sync::mpsc::channel::<Value>();
    thread::spawn(move || {
        for response in receiver {
            let Ok(encoded) = serde_json::to_string(&HostBridgeResponse { response }) else {
                continue;
            };
            let line = format!("{encoded}\n");
            if !pipe_write(pipe, line.as_bytes()) {
                break;
            }
        }
    });

    let mut registered_key: Option<String> = None;
    loop {
        let Some(message) = pipe_read_message(pipe) else {
            break;
        };
        let lossy = String::from_utf8_lossy(&message);
        let line = lossy.trim_end_matches('\n');
        let (response, connection_key) =
            dispatch_inbound(&coordinator, line, &capability_token, writer.clone());
        if let Some(key) = connection_key {
            registered_key = Some(key);
        }
        if writer.send(response).is_err() {
            break;
        }
    }
    close_pipe(pipe);
    // The browser session is gone: drop its port registration and the tabs it
    // reported so the App never offers stale targets.
    if let Some(key) = &registered_key {
        let mut coordinator = coordinator.lock();
        coordinator.browser_ports.remove(key);
        let prefix = format!("{key}:");
        coordinator
            .connected_tabs
            .retain(|target_key, _| !target_key.starts_with(&prefix));
    }
}

/// The transport-agnostic message dispatch: parse a `HostBridgeRequest`,
/// enforce the capability token, and produce a response `Value`.
///
/// The second result item is the connection key this message registered
/// (a successful `hello`), so the serving loop can clean the registration up
/// when the connection drops.
fn dispatch_inbound(
    coordinator: &Arc<Mutex<DispatchCoordinator>>,
    line: &str,
    capability_token: &str,
    writer: Sender<Value>,
) -> (Value, Option<String>) {
    match serde_json::from_str::<HostBridgeRequest>(line) {
        Ok(request) if request.capability_token == capability_token => {
            handle_message(coordinator, request.message.message, writer)
        }
        Ok(_) => {
            record_rejection(coordinator, "rejected_disconnected");
            (
                json!({ "type": "error", "code": "rejected_disconnected", "detail": "native host capability token invalid" }),
                None,
            )
        }
        Err(error) => {
            record_rejection(coordinator, "invalid_message");
            (
                json!({ "type": "error", "code": "invalid_message", "detail": error.to_string() }),
                None,
            )
        }
    }
}

/// Record the last rejected handshake so the UI can tell the user which
/// stage of the native-host chain is broken.
fn record_rejection(coordinator: &Arc<Mutex<DispatchCoordinator>>, code: &str) {
    coordinator.lock().last_bridge_rejection = Some(code.to_owned());
}

fn handle_message(
    coordinator: &Arc<Mutex<DispatchCoordinator>>,
    message: Value,
    host_sender: Sender<Value>,
) -> (Value, Option<String>) {
    match message.get("type").and_then(Value::as_str) {
        Some("hello") => match serde_json::from_value::<Hello>(message) {
            Ok(hello)
                if hello.message_type == "hello" && hello.protocol_major == PROTOCOL_MAJOR =>
            {
                let key = connection_key(
                    &hello.browser,
                    &hello.browser_instance_id,
                    &hello.session_nonce,
                );
                let mut coordinator = coordinator.lock();
                coordinator.browser_ports.insert(key.clone(), host_sender);
                coordinator.last_bridge_rejection = None;
                coordinator.last_host_disconnect_reason = hello.last_disconnect_reason;
                (json!({ "type": "ack", "protocol_major": PROTOCOL_MAJOR }), Some(key))
            }
            Ok(_) => {
                record_rejection(coordinator, "protocol_mismatch");
                (json!({ "type": "error", "code": "protocol_mismatch" }), None)
            }
            Err(error) => {
                record_rejection(coordinator, "invalid_message");
                (
                    json!({ "type": "error", "code": "invalid_message", "detail": error.to_string() }),
                    None,
                )
            }
        },
        Some("tabs_snapshot") => match serde_json::from_value::<TabsSnapshot>(message) {
            Ok(snapshot) if snapshot.message_type == "tabs_snapshot" => {
                let mut coordinator = coordinator.lock();
                coordinator.connected_tabs.retain(|_, tab| {
                    tab.target.browser_instance_id != snapshot.browser_instance_id
                        || tab.target.session_nonce != snapshot.session_nonce
                });
                for tab in snapshot.tabs {
                    if tab.target.browser_instance_id != snapshot.browser_instance_id
                        || tab.target.session_nonce != snapshot.session_nonce
                    {
                        return (
                            json!({ "type": "error", "code": "invalid_message", "detail": "tabs_snapshot and session identity disagree" }),
                            None,
                        );
                    }
                    coordinator
                        .connected_tabs
                        .insert(target_key(&tab.target), tab);
                }
                (json!({ "type": "ack", "protocol_major": PROTOCOL_MAJOR }), None)
            }
            Ok(_) => {
                record_rejection(coordinator, "invalid_message");
                (json!({ "type": "error", "code": "invalid_message" }), None)
            }
            Err(error) => {
                record_rejection(coordinator, "invalid_message");
                (
                    json!({ "type": "error", "code": "invalid_message", "detail": error.to_string() }),
                    None,
                )
            }
        },
        Some("prepared") => match serde_json::from_value::<PreparedResult>(message) {
            Ok(prepared) if prepared.message_type == "prepared" => {
                let pending = coordinator
                    .lock()
                    .pending_prepares
                    .remove(&prepared.request_id);
                if let Some(sender) = pending {
                    let _ = sender.send(prepared);
                }
                (json!({ "type": "ack", "protocol_major": PROTOCOL_MAJOR }), None)
            }
            Ok(_) => {
                record_rejection(coordinator, "invalid_message");
                (json!({ "type": "error", "code": "invalid_message" }), None)
            }
            Err(error) => {
                record_rejection(coordinator, "invalid_message");
                (
                    json!({ "type": "error", "code": "invalid_message", "detail": error.to_string() }),
                    None,
                )
            }
        },
        _ => {
            record_rejection(coordinator, "unsupported_message");
            (
                json!({ "type": "error", "code": "unsupported_message", "detail": "unsupported native host message type" }),
                None,
            )
        }
    }
}

pub(crate) fn connection_key(
    browser: &BrowserKind,
    browser_instance_id: &str,
    session_nonce: &str,
) -> String {
    format!("{browser}:{browser_instance_id}:{session_nonce}")
}

pub(crate) fn connection_key_for_target(target: &TabTarget) -> String {
    connection_key(
        &target.browser,
        &target.browser_instance_id,
        &target.session_nonce,
    )
}

#[cfg(target_os = "windows")]
mod named_pipe {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{
            CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_FIRST_PIPE_INSTANCE, OPEN_EXISTING,
            ReadFile, WriteFile,
        },
        System::Pipes::{
            ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_MESSAGE, PIPE_TYPE_MESSAGE,
            PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
        },
    };

    const PIPE_BUFFER_BYTES: usize = 2 * 1024 * 1024;

    /// A named pipe handle. `HANDLE` is a raw pointer (`*mut c_void`), which is
    /// `!Send`, but a Windows handle is a process-wide value, not thread-local,
    /// so moving the handle between threads is safe. The value is `Copy` so it
    /// can be shared across the connect/serve/close call sites.
    #[derive(Clone, Copy)]
    #[repr(transparent)]
    pub(super) struct PipeHandle(windows_sys::Win32::Foundation::HANDLE);
    unsafe impl Send for PipeHandle {}

    pub(super) fn create_named_pipe(wide: &[u16], first: bool) -> Option<PipeHandle> {
        let flags = (GENERIC_READ | GENERIC_WRITE)
            | if first {
                FILE_FLAG_FIRST_PIPE_INSTANCE
            } else {
                0
            };
        let handle = unsafe {
            CreateNamedPipeW(
                wide.as_ptr(),
                flags,
                PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT,
                PIPE_UNLIMITED_INSTANCES,
                PIPE_BUFFER_BYTES as u32,
                PIPE_BUFFER_BYTES as u32,
                0,
                std::ptr::null(),
            )
        };
        (handle != INVALID_HANDLE_VALUE).then_some(PipeHandle(handle))
    }

    pub(super) fn connect_named_pipe(handle: PipeHandle) -> bool {
        unsafe { ConnectNamedPipe(handle.0, std::ptr::null_mut()) != 0 }
    }

    pub(super) fn close_pipe(handle: PipeHandle) {
        unsafe {
            CloseHandle(handle.0);
        }
    }

    pub(super) fn pipe_read_message(handle: PipeHandle) -> Option<Vec<u8>> {
        let mut buffer = vec![0_u8; PIPE_BUFFER_BYTES];
        let mut read = 0_u32;
        let ok = unsafe {
            ReadFile(
                handle.0,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                &mut read,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 || read == 0 {
            return None;
        }
        buffer.truncate(read as usize);
        Some(buffer)
    }

    pub(super) fn pipe_write(handle: PipeHandle, bytes: &[u8]) -> bool {
        let mut written = 0_u32;
        let ok = unsafe {
            WriteFile(
                handle.0,
                bytes.as_ptr(),
                bytes.len() as u32,
                &mut written,
                std::ptr::null_mut(),
            )
        };
        ok != 0
    }

    /// Connects to an existing named pipe (used by the native host client).
    #[allow(dead_code)]
    pub(super) fn open_existing_pipe(name: &str) -> Option<PipeHandle> {
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
        (handle != INVALID_HANDLE_VALUE).then_some(PipeHandle(handle))
    }
}

#[cfg(target_os = "windows")]
use named_pipe::{
    close_pipe, connect_named_pipe, create_named_pipe, pipe_read_message, pipe_write,
};

#[cfg(unix)]
#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::{BufRead, Write},
        os::unix::net::UnixStream,
        path::Path,
        time::Duration,
    };

    use super::*;
    use banana_hand_protocol::NativeHostMessage;
    use std::path::PathBuf;

    struct TestBridge {
        socket_path: PathBuf,
        capability_token: String,
        runtime_directory: PathBuf,
    }

    fn setup_bridge(
        coordinator: &Arc<Mutex<crate::DispatchCoordinator>>,
        runtime_directory: &Path,
    ) -> TestBridge {
        let _ = fs::remove_dir_all(runtime_directory);
        unsafe { std::env::set_var("XDG_RUNTIME_DIR", runtime_directory) };

        start(coordinator.clone()).expect("bridge should start successfully");

        let config_path = runtime_directory.join("banana-hand").join("bridge.json");
        let config: NativeHostBridgeConfig =
            serde_json::from_str(&fs::read_to_string(config_path).unwrap()).unwrap();

        TestBridge {
            socket_path: config
                .socket_path
                .expect("unix config should carry a socket path"),
            capability_token: config.capability_token,
            runtime_directory: runtime_directory.to_path_buf(),
        }
    }

    fn cleanup_bridge(bridge: &TestBridge) {
        let _ = fs::remove_dir_all(&bridge.runtime_directory);
        unsafe { std::env::remove_var("XDG_RUNTIME_DIR") };
    }

    fn connect(bridge: &TestBridge) -> UnixStream {
        UnixStream::connect(&bridge.socket_path).expect("should connect to the bridge socket")
    }

    fn send_message(stream: &mut UnixStream, bridge: &TestBridge, message: &Value) -> Value {
        let request = HostBridgeRequest {
            capability_token: bridge.capability_token.clone(),
            message: NativeHostMessage {
                request_id: "test-req".into(),
                message: message.clone(),
            },
        };
        serde_json::to_writer(&mut *stream, &request).unwrap();
        stream.write_all(b"\n").unwrap();
        stream.flush().unwrap();
        let mut line = String::new();
        let _ = BufReader::new(stream).read_line(&mut line);
        let response: HostBridgeResponse = serde_json::from_str(&line).unwrap();
        response.response
    }

    fn tab_target(instance: &str, nonce: &str, tab_id: u32) -> Value {
        json!({
            "browser": "chrome",
            "browser_instance_id": instance,
            "session_nonce": nonce,
            "window_id": 1,
            "tab_id": tab_id,
            "generation": 0
        })
    }

    // Two tests in this module both mutate the process-wide XDG_RUNTIME_DIR;
    // serialize them so neither observes the other's environment or cleanup.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn rejects_wrong_token_and_unknown_protocol_major() {
        let _env_guard = ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let runtime_directory =
            std::env::temp_dir().join(format!("banana-hand-test-{}-a", std::process::id()));
        let coordinator = Arc::new(Mutex::new(crate::DispatchCoordinator::default()));
        let bridge = setup_bridge(&coordinator, &runtime_directory);

        let mut stream = connect(&bridge);

        // Wrong token -> rejected_disconnected.
        let request = HostBridgeRequest {
            capability_token: "wrong-token".into(),
            message: NativeHostMessage {
                request_id: "test-req".into(),
                message: json!({"type": "hello"}),
            },
        };
        serde_json::to_writer(&mut stream, &request).unwrap();
        stream.write_all(b"\n").unwrap();
        stream.flush().unwrap();
        let mut line = String::new();
        let _ = BufReader::new(&stream).read_line(&mut line);
        let response: HostBridgeResponse = serde_json::from_str(&line).unwrap();
        assert_eq!(
            response.response.get("code").and_then(Value::as_str),
            Some("rejected_disconnected")
        );

        // Right token, wrong protocol major -> protocol_mismatch.
        let mut stream = connect(&bridge);
        let response = send_message(
            &mut stream,
            &bridge,
            &json!({
                "type": "hello",
                "protocol_major": 99,
                "browser": "chrome",
                "browser_instance_id": "instance-x",
                "session_nonce": "nonce-x"
            }),
        );
        assert_eq!(
            response.get("code").and_then(Value::as_str),
            Some("protocol_mismatch")
        );

        cleanup_bridge(&bridge);
    }

    #[test]
    fn relays_tabs_snapshot_and_prepared_results() {
        let _env_guard = ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let runtime_directory =
            std::env::temp_dir().join(format!("banana-hand-test-{}-b", std::process::id()));
        let coordinator = Arc::new(Mutex::new(crate::DispatchCoordinator::default()));
        let bridge = setup_bridge(&coordinator, &runtime_directory);

        let mut stream = connect(&bridge);

        // hello registers the browser port.
        let hello = json!({
            "type": "hello",
            "protocol_major": PROTOCOL_MAJOR,
            "browser": "chrome",
            "browser_instance_id": "instance-a",
            "session_nonce": "nonce-a"
        });
        let response = send_message(&mut stream, &bridge, &hello);
        assert_eq!(response.get("type").and_then(Value::as_str), Some("ack"));

        // tabs_snapshot populates connected_tabs.
        let snapshot = json!({
            "type": "tabs_snapshot",
            "browser_instance_id": "instance-a",
            "session_nonce": "nonce-a",
            "tabs": [
                { "target": tab_target("instance-a", "nonce-a", 1), "title": "Tab A", "url": "https://example.com/a" },
                { "target": tab_target("instance-a", "nonce-a", 2), "title": "Tab B", "url": "https://example.com/b" }
            ]
        });
        let response = send_message(&mut stream, &bridge, &snapshot);
        assert_eq!(response.get("type").and_then(Value::as_str), Some("ack"));

        {
            let coordinator_locked = coordinator.lock();
            assert_eq!(coordinator_locked.connected_tabs.len(), 2);
            assert!(
                coordinator_locked
                    .browser_ports
                    .contains_key("Chrome:instance-a:nonce-a")
            );
        }

        // prepared relays back through the pending channel.
        let (tx, rx) = std::sync::mpsc::channel();
        {
            let mut coordinator_locked = coordinator.lock();
            coordinator_locked
                .pending_prepares
                .insert("request-1".to_string(), tx);
        }
        let prepared = json!({
            "type": "prepared",
            "request_id": "request-1",
            "ready": true,
            "code": null,
            "detail": null
        });
        let response = send_message(&mut stream, &bridge, &prepared);
        assert_eq!(response.get("type").and_then(Value::as_str), Some("ack"));

        let relayed = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(relayed.ready);
        assert_eq!(relayed.request_id, "request-1");

        // The pending entry is consumed exactly once.
        {
            let coordinator_locked = coordinator.lock();
            assert!(
                !coordinator_locked
                    .pending_prepares
                    .contains_key("request-1")
            );
        }

        cleanup_bridge(&bridge);
    }

    // ---- Real-process integration --------------------------------------
    // The unit tests above drive the App half with a *simulated* host (a raw
    // UnixStream). These spawn the actual `banana-hand-native-host` binary and
    // let it bridge a browser-style native-messaging client to the live App
    // socket, exercising the runtime chain those tests cannot reach.

    /// Locate the native host binary in the shared workspace target dir. It is
    /// built by `cargo test --workspace`; if a narrower invocation left it out,
    /// fail with an explicit instruction rather than a mysterious spawn error.
    fn native_host_binary() -> PathBuf {
        let target_dir = std::env::var("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("..")
                    .join("target")
            });
        let bin = target_dir.join("debug").join("banana-hand-native-host");
        assert!(
            bin.exists(),
            "native host binary not found at {bin:?}; run `cargo build -p banana-hand-native-host` first"
        );
        bin
    }

    /// Native-messaging wire format: a little-endian 4-byte length prefix
    /// followed by a JSON body. Mirrors the native host's
    /// `read_native_message`/`write_native_message` so this test speaks the
    /// exact framing the browser uses.
    fn write_framed(writer: &mut impl Write, msg: &Value) {
        let body = serde_json::to_vec(msg).unwrap();
        writer
            .write_all(&(body.len() as u32).to_le_bytes())
            .unwrap();
        writer.write_all(&body).unwrap();
        writer.flush().unwrap();
    }

    fn read_framed(reader: &mut impl std::io::Read) -> Option<Value> {
        let mut len = [0_u8; 4];
        match reader.read_exact(&mut len) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return None,
            Err(e) => panic!("short read on native host stdout: {e}"),
        }
        let len = u32::from_le_bytes(len) as usize;
        let mut body = vec![0_u8; len];
        reader.read_exact(&mut body).unwrap();
        serde_json::from_slice(&body).ok()
    }

    #[test]
    fn integration_real_host_relays_browser_hello_to_app_and_back() {
        let _env_guard = ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let runtime_directory =
            std::env::temp_dir().join(format!("banana-hand-e2e-{}-c", std::process::id()));
        let coordinator = Arc::new(Mutex::new(crate::DispatchCoordinator::default()));
        let bridge = setup_bridge(&coordinator, &runtime_directory);

        // Spawn the real native host. It inherits XDG_RUNTIME_DIR, reads
        // bridge.json, and connects to the live App socket as the browser would.
        let mut child = std::process::Command::new(native_host_binary())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn the native host");

        // Give the host a moment to open its socket connection.
        std::thread::sleep(Duration::from_millis(300));

        // Speak as the browser: a native-messaging-framed "hello".
        let hello = json!({
            "type": "hello",
            "protocol_major": PROTOCOL_MAJOR,
            "browser": "chrome",
            "browser_instance_id": "e2e-instance",
            "session_nonce": "e2e-nonce",
            "last_disconnect_reason": "Native messaging host not found"
        });
        let mut host_stdin = child.stdin.take().expect("host stdin");
        write_framed(&mut host_stdin, &hello);

        // The real host must relay it to the App, which registers the browser port.
        let key = "Chrome:e2e-instance:e2e-nonce";
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut registered = false;
        loop {
            if coordinator.lock().browser_ports.contains_key(key) {
                registered = true;
                break;
            }
            if std::time::Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(
            registered,
            "real native host did not relay the browser hello to the App"
        );
        // The extension's own diagnosis of its previous failed connect must
        // reach the coordinator so the UI can show it.
        assert_eq!(
            coordinator.lock().last_host_disconnect_reason.as_deref(),
            Some("Native messaging host not found")
        );

        // The App's ack must come back out the host's stdout, native-messaging framed.
        let mut host_stdout = child.stdout.take().expect("host stdout");
        let ack = read_framed(&mut host_stdout)
            .expect("host closed stdout without returning the App's ack");
        assert_eq!(ack.get("type").and_then(Value::as_str), Some("ack"));
        assert_eq!(
            ack.get("protocol_major").and_then(Value::as_u64),
            Some(u64::from(PROTOCOL_MAJOR))
        );

        let _ = child.kill();
        let _ = child.wait();
        cleanup_bridge(&bridge);
    }

    #[test]
    fn integration_real_host_fails_closed_when_app_absent() {
        let _env_guard = ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let runtime_directory =
            std::env::temp_dir().join(format!("banana-hand-e2e-{}-d", std::process::id()));
        let _ = fs::remove_dir_all(&runtime_directory);
        let dir = runtime_directory.join("banana-hand");
        fs::create_dir_all(&dir).unwrap();
        unsafe { std::env::set_var("XDG_RUNTIME_DIR", &runtime_directory) };
        // Config names a socket that does not exist (the App is not running).
        let config = NativeHostBridgeConfig {
            socket_path: Some(dir.join("absent.sock")),
            pipe_name: None,
            capability_token: "irrelevant".into(),
        };
        fs::write(
            dir.join("bridge.json"),
            serde_json::to_vec(&config).unwrap(),
        )
        .unwrap();

        let mut child = std::process::Command::new(native_host_binary())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn the native host");

        // Fail-closed: the host must exit (non-zero) rather than hang or
        // masquerade as a successful launch.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut output = None;
        loop {
            match child.try_wait().expect("wait on the native host") {
                Some(_) => {
                    output = Some(
                        child
                            .wait_with_output()
                            .expect("collect native host output"),
                    );
                    break;
                }
                None => {
                    if std::time::Instant::now() > deadline {
                        let _ = child.kill();
                        panic!(
                            "native host did not fail closed; it kept running with the App absent"
                        );
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        }
        let out = output.expect("collected native host output");
        assert!(
            !out.status.success(),
            "native host exited 0 despite being unable to reach the App (fail-open)"
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("banana-hand-native-host:"),
            "expected a native host error on stderr, got: {stderr}"
        );

        unsafe { std::env::remove_var("XDG_RUNTIME_DIR") };
        let _ = fs::remove_dir_all(&runtime_directory);
    }
}
