#![cfg(any(target_os = "linux", target_os = "macos"))]

use std::{
    fs,
    io::{Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    process::{Child, Command, ExitStatus, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    thread,
    time::{Duration, Instant},
};

use banana_hand_protocol::NativeHostBridgeConfig;
use serde_json::{Value, json};

const TIMEOUT: Duration = Duration::from_secs(5);
static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

struct HostProcess {
    directory: PathBuf,
    child: Option<Child>,
}

impl HostProcess {
    fn start() -> (Self, UnixStream) {
        let directory = std::env::temp_dir().join(format!(
            "bh-host-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let mut host = Self {
            directory,
            child: None,
        };
        #[cfg(target_os = "linux")]
        let runtime_directory = host.directory.join("banana-hand");
        #[cfg(target_os = "macos")]
        let runtime_directory = host.directory.join("Library/Caches/Banana Hand/runtime");
        fs::create_dir_all(&runtime_directory).unwrap();
        let socket_path = host.directory.join("bridge.sock");
        let listener = UnixListener::bind(&socket_path).unwrap();
        listener.set_nonblocking(true).unwrap();
        let config = NativeHostBridgeConfig {
            socket_path: Some(socket_path),
            pipe_name: None,
            capability_token: "relay-test-token".into(),
        };
        fs::write(
            runtime_directory.join("bridge.json"),
            serde_json::to_vec(&config).unwrap(),
        )
        .unwrap();
        host.child = Some(
            Command::new(env!("CARGO_BIN_EXE_banana-hand-native-host"))
                .env("XDG_RUNTIME_DIR", &host.directory)
                .env("HOME", &host.directory)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + TIMEOUT;
        let desktop = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        host.child.as_mut().unwrap().try_wait().unwrap().is_none(),
                        "native host exited before connecting to the desktop"
                    );
                    assert!(Instant::now() < deadline, "desktop connection timed out");
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("desktop accept failed: {error}"),
            }
        };
        desktop.set_write_timeout(Some(TIMEOUT)).unwrap();
        (host, desktop)
    }

    fn wait_for_exit(&mut self) -> ExitStatus {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Some(status) = self.child.as_mut().unwrap().try_wait().unwrap() {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "native host stayed alive after the desktop disconnected with browser stdin open"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn responses(&mut self) -> Vec<Value> {
        // Only called after process exit, so a missing frame cannot block the test.
        let mut bytes = Vec::new();
        self.child
            .as_mut()
            .unwrap()
            .stdout
            .as_mut()
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        let mut remaining = bytes.as_slice();
        let mut responses = Vec::new();
        while !remaining.is_empty() {
            assert!(remaining.len() >= 4, "truncated native frame header");
            let length = u32::from_le_bytes(remaining[..4].try_into().unwrap()) as usize;
            remaining = &remaining[4..];
            assert!(remaining.len() >= length, "truncated native frame body");
            responses.push(serde_json::from_slice(&remaining[..length]).unwrap());
            remaining = &remaining[length..];
        }
        responses
    }
}

impl Drop for HostProcess {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn desktop_close_exits_while_browser_stdin_remains_open() {
    let (mut host, desktop) = HostProcess::start();
    drop(desktop);
    assert!(!host.wait_for_exit().success());
}

#[test]
fn coalesced_desktop_responses_are_relayed_before_exit() {
    let (mut host, mut desktop) = HostProcess::start();
    let responses = [
        json!({"request_id": "first", "tabs": []}),
        json!({"request_id": "second", "connected": true}),
        json!({"request_id": "third", "title": "香蕉手"}),
    ];
    let mut batch = Vec::new();
    for response in &responses {
        serde_json::to_writer(&mut batch, &json!({"response": response})).unwrap();
        batch.push(b'\n');
    }
    desktop.write_all(&batch).unwrap();
    drop(desktop);
    assert!(!host.wait_for_exit().success());
    assert_eq!(host.responses(), responses);
}

#[test]
fn desktop_read_error_exits_while_browser_stdin_remains_open() {
    let (mut host, mut desktop) = HostProcess::start();
    // Invalid UTF-8 makes the newline reader fail without closing the socket.
    desktop.write_all(b"\xff\n").unwrap();
    assert!(!host.wait_for_exit().success());
}
