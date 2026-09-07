use std::{
    io::{Read, Write},
    os::windows::io::AsRawHandle,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant},
};

use super::*;

const IO_TIMEOUT: Duration = Duration::from_secs(5);
const IDLE: Duration = Duration::from_millis(250);
const MAX_FRAME_BYTES: usize = 1_048_576;

type BrowserWrite = (Value, Sender<Result<(), String>>);

struct TestBridge {
    local_app_data: PathBuf,
    host: Option<Child>,
    browser_input: Option<Sender<BrowserWrite>>,
    browser_output: Receiver<Result<Value, String>>,
    server: Option<thread::JoinHandle<()>>,
    server_done: Receiver<()>,
    coordinator: Arc<Mutex<DispatchCoordinator>>,
}

impl TestBridge {
    fn start() -> Self {
        let suffix = format!("{}-{:016x}", std::process::id(), rand::random::<u64>());
        let local_app_data = std::env::temp_dir().join(format!("banana-hand-pipe-{suffix}"));
        // Match protocol::local_runtime_directory without mutating this process's env.
        let runtime_directory = local_app_data.join("Banana Hand").join("runtime");
        let pipe_name = format!(r"\\.\pipe\banana-hand-test-{suffix}");
        let capability_token = generate_capability_token();
        let coordinator = Arc::new(Mutex::new(DispatchCoordinator::default()));
        let (output_sender, browser_output) = mpsc::channel();
        let (done_sender, server_done) = mpsc::channel();
        let mut bridge = Self {
            local_app_data,
            host: None,
            browser_input: None,
            browser_output,
            server: None,
            server_done,
            coordinator: coordinator.clone(),
        };
        fs::create_dir_all(&runtime_directory).expect("create isolated host runtime directory");
        write_bridge_config(
            &runtime_directory,
            &NativeHostBridgeConfig {
                socket_path: None,
                pipe_name: Some(pipe_name.clone()),
                capability_token: capability_token.clone(),
            },
        )
        .expect("publish isolated real host config");

        let pipe_wide: Vec<u16> = pipe_name.encode_utf16().chain(Some(0)).collect();
        let pipe = create_named_pipe(&pipe_wide, true).unwrap_or_else(|| {
            panic!(
                "create real named pipe: {}",
                std::io::Error::last_os_error()
            )
        });
        let (accept_sender, accept_receiver) = mpsc::channel();
        bridge.server = Some(thread::spawn(move || {
            let _ = accept_sender.send(());
            if connect_named_pipe(pipe) {
                serve_pipe(pipe, coordinator, capability_token);
            } else {
                eprintln!(
                    "[windows-bridge] ConnectNamedPipe failed: {}",
                    std::io::Error::last_os_error()
                );
                close_pipe(pipe);
            }
            let _ = done_sender.send(());
        }));
        accept_receiver
            .recv_timeout(IO_TIMEOUT)
            .expect("start real pipe accept thread");
        // Give the blocking accept a head start before launching the client process.
        thread::sleep(IDLE);

        // Cargo test executables live in <profile>/deps, including --target builds.
        let test_binary = std::env::current_exe().expect("locate test executable");
        let host_binary = test_binary
            .parent()
            .expect("test deps directory")
            .parent()
            .expect("Cargo profile directory")
            .join("banana-hand-native-host.exe");
        assert!(
            host_binary.is_file(),
            "build the real host first: cargo build -p banana-hand-native-host (use the same --target/profile as cargo test); missing {}",
            host_binary.display()
        );
        eprintln!("[windows-bridge] launching {}", host_binary.display());
        bridge.host = Some(
            Command::new(&host_binary)
                .env("LOCALAPPDATA", &bridge.local_app_data)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .expect("launch real native host"),
        );
        let host = bridge.host.as_mut().unwrap();
        let mut stdin = host.stdin.take().expect("native host stdin");
        let mut stdout = host.stdout.take().expect("native host stdout");
        let (input_sender, input_receiver) = mpsc::channel::<BrowserWrite>();
        bridge.browser_input = Some(input_sender);
        thread::spawn(move || {
            // Keep stdin OPEN while waiting: EOF would terminate the real host and
            // fail to exercise unsolicited desktop -> browser prepare traffic.
            for (message, completed) in input_receiver {
                let result = write_framed(&mut stdin, &message);
                let failed = result.is_err();
                let _ = completed.send(result);
                if failed {
                    break;
                }
            }
        });
        thread::spawn(move || {
            loop {
                let result = read_framed(&mut stdout);
                let failed = result.is_err();
                if output_sender.send(result).is_err() || failed {
                    break;
                }
            }
        });
        bridge
    }

    fn diagnostics(&mut self) -> String {
        let status = self.host.as_mut().map(Child::try_wait);
        match self.coordinator.try_lock_for(Duration::from_millis(100)) {
            Some(state) => format!(
                "host={status:?}; registered_hosts={}; tabs={}; pending_prepares={}; rejection={:?}",
                state.browser_ports.len(),
                state.connected_tabs.len(),
                state.pending_prepares.len(),
                state.last_bridge_rejection,
            ),
            None => format!("host={status:?}; coordinator lock remained busy"),
        }
    }

    fn send(&mut self, message: Value, stage: &str) {
        let (completed, result) = mpsc::channel();
        self.browser_input
            .as_ref()
            .unwrap()
            .send((message, completed))
            .unwrap_or_else(|_| panic!("{stage}: browser stdin worker stopped"));
        match result.recv_timeout(IO_TIMEOUT) {
            Ok(Ok(())) => {}
            other => panic!(
                "{stage}: native stdin write failed or stalled: {other:?}; {}",
                self.diagnostics()
            ),
        }
    }

    fn receive(&mut self, stage: &str) -> Value {
        match self.browser_output.recv_timeout(IO_TIMEOUT) {
            Ok(Ok(message)) => message,
            other => panic!(
                "{stage}: no complete native stdout frame within {IO_TIMEOUT:?}: {other:?}; {}",
                self.diagnostics()
            ),
        }
    }

    fn ack(&mut self, stage: &str) {
        assert_eq!(
            self.receive(stage),
            json!({"type": "ack", "protocol_major": PROTOCOL_MAJOR}),
            "{stage}"
        );
        eprintln!("[windows-bridge] {stage}: ack returned through real host");
    }
}

impl Drop for TestBridge {
    fn drop(&mut self) {
        // Kill only the child we launched, even on assertion panic. Never use an
        // unbounded wait/join: this test must fail rather than hang on a deadlock.
        self.browser_input.take();
        if let Some(host) = &mut self.host {
            let _ = host.kill();
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                match host.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if Instant::now() < deadline => {
                        thread::sleep(Duration::from_millis(10))
                    }
                    result => {
                        eprintln!("[windows-bridge] child cleanup did not complete: {result:?}");
                        break;
                    }
                }
            }
        }
        if let Some(server) = self.server.take() {
            // Also release an accept/read if the host failed before connecting.
            unsafe {
                windows_sys::Win32::System::IO::CancelSynchronousIo(server.as_raw_handle());
            }
            if self
                .server_done
                .recv_timeout(Duration::from_secs(2))
                .is_err()
            {
                eprintln!("[windows-bridge] server cleanup did not complete within 2s");
            }
        }
        let _ = fs::remove_dir_all(&self.local_app_data);
    }
}

fn write_framed(writer: &mut impl Write, message: &Value) -> Result<(), String> {
    let body = serde_json::to_vec(message).map_err(|error| error.to_string())?;
    if body.len() > MAX_FRAME_BYTES {
        return Err("native frame exceeds 1 MiB".into());
    }
    writer
        .write_all(&(body.len() as u32).to_le_bytes())
        .and_then(|()| writer.write_all(&body))
        .and_then(|()| writer.flush())
        .map_err(|error| error.to_string())
}

fn read_framed(reader: &mut impl Read) -> Result<Value, String> {
    let mut length = [0_u8; 4];
    reader
        .read_exact(&mut length)
        .map_err(|error| format!("native frame header: {error}"))?;
    let length = u32::from_le_bytes(length) as usize;
    if length > MAX_FRAME_BYTES {
        return Err(format!("native frame length {length} exceeds 1 MiB"));
    }
    let mut body = vec![0_u8; length];
    reader
        .read_exact(&mut body)
        .map_err(|error| format!("native frame body ({length} bytes): {error}"))?;
    serde_json::from_slice(&body).map_err(|error| format!("native frame JSON: {error}"))
}

#[test]
fn real_host_prepares_targets_while_both_pipe_readers_are_idle() {
    let mut bridge = TestBridge::start();
    let target = TabTarget {
        browser: BrowserKind::Chrome,
        browser_instance_id: "windows-pipe-instance".into(),
        session_nonce: "windows-pipe-session".into(),
        window_id: 1,
        tab_id: 2,
        generation: 1,
    };
    bridge.send(
        json!({
            "type": "hello", "protocol_major": PROTOCOL_MAJOR,
            "browser": target.browser, "browser_instance_id": target.browser_instance_id,
            "session_nonce": target.session_nonce,
        }),
        "hello request",
    );
    bridge.ack("hello response");
    bridge.send(
        json!({
            "type": "tabs_snapshot", "browser_instance_id": target.browser_instance_id,
            "session_nonce": target.session_nonce,
            "tabs": [{ "target": target, "title": "Windows bridge regression", "url": null }],
        }),
        "snapshot request",
    );
    bridge.ack("snapshot response");

    for attempt in 1..=3 {
        // No browser writes during this interval: the production desktop reader
        // is waiting for its next request, and the host reader for a response.
        thread::sleep(IDLE);
        let coordinator = bridge.coordinator.clone();
        let prepare_target = target.clone();
        let parent_request_id = format!("windows-idle-{attempt}");
        let expected_id = format!("{parent_request_id}:{}", target_key(&target));
        let (prepared_sender, prepared_receiver) = mpsc::channel();
        thread::spawn(move || {
            let result = crate::prepare_target(&coordinator, &prepare_target, &parent_request_id);
            eprintln!("[windows-bridge] prepare #{attempt} completed: {result:?}");
            let _ = prepared_sender.send(result);
        });
        let prepare = bridge.receive(&format!("prepare #{attempt}: desktop -> idle browser"));
        assert_eq!(
            prepare,
            json!({"type": "prepare", "request_id": expected_id, "target": target}),
            "prepare #{attempt}: wrong request or frame ordering"
        );
        eprintln!("[windows-bridge] prepare #{attempt} reached idle browser");

        // Allow the host's desktop ReadFile to pend again before its browser
        // relay must write the prepared result on the same duplex connection.
        thread::sleep(IDLE);
        bridge.send(
            json!({
                "type": "prepared", "request_id": expected_id,
                "ready": true, "code": null, "detail": null,
            }),
            &format!("prepared #{attempt}: browser -> idle desktop"),
        );
        let result = prepared_receiver.recv_timeout(IO_TIMEOUT)
            .unwrap_or_else(|error| panic!("prepare #{attempt}: real prepare_target failed to finish: {error}; {}", bridge.diagnostics()))
            .unwrap_or_else(|error| panic!("prepare #{attempt}: real prepare_target rejected prepared response: {error}; {}", bridge.diagnostics()));
        assert!(
            result.ready,
            "prepare #{attempt}: expected verified ready result"
        );
        assert_eq!(
            result.request_id, expected_id,
            "prepare #{attempt}: mismatched result"
        );
        bridge.ack(&format!("prepared #{attempt} response"));
    }
}
