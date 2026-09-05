mod bridge;
mod input;
mod native_host;

use std::{
    collections::HashMap,
    fs,
    sync::{Arc, mpsc::Sender},
    time::{Duration, Instant},
};

use banana_hand_protocol::{
    AttemptStatus, DispatchAttempt, DispatchOutcome, DispatchRequest, TabMetadata, TabTarget,
};
use input::{InputAdapter, PlatformInputAdapter};
use serde::Serialize;

use parking_lot::Mutex;
use serde_json::{Value, json};
use tauri::{Manager, State};

const COOLDOWN: Duration = Duration::from_secs(60);
const PREPARE_TIMEOUT: Duration = Duration::from_secs(3);

struct AppState {
    coordinator: Arc<Mutex<DispatchCoordinator>>,
    input_adapter: PlatformInputAdapter,
    /// The native-host auto-registration outcome, computed once on startup.
    native_host_registration: Mutex<native_host::AutoRegisterResult>,
    /// The one-shot `--self-check` result of the sidecar binary, computed on
    /// startup and surfaced in the status line when no extension is
    /// connected.
    host_self_check: Mutex<Option<String>>,
}

#[derive(Default)]
pub(crate) struct DispatchCoordinator {
    pub(crate) connected_tabs: HashMap<String, TabMetadata>,
    pub(crate) browser_ports: HashMap<String, Sender<Value>>,
    pub(crate) pending_prepares: HashMap<String, Sender<bridge::PreparedResult>>,
    cooldown_started_at: Option<Instant>,
    /// The last rejected native-host handshake code (e.g. "protocol_mismatch");
    /// cleared when a hello succeeds.
    pub(crate) last_bridge_rejection: Option<String>,
    /// The last disconnect reason the extension itself reported in its hello
    /// (the browser's own words, e.g. "Native messaging host not found").
    pub(crate) last_host_disconnect_reason: Option<String>,
}

impl DispatchCoordinator {
    fn cooldown_remaining_seconds(&self) -> u64 {
        self.cooldown_started_at
            .and_then(|started_at| COOLDOWN.checked_sub(started_at.elapsed()))
            .map(|remaining| remaining.as_secs().saturating_add(1))
            .unwrap_or(0)
    }
}

#[derive(Serialize)]
struct RuntimeSnapshot {
    tabs: Vec<TabMetadata>,
    cooldown_remaining_seconds: u64,
    connected_hosts: u32,
    last_bridge_rejection: Option<String>,
    last_host_disconnect_reason: Option<String>,
    host_self_check: Option<String>,
}

#[tauri::command]
fn runtime_snapshot(state: State<'_, AppState>) -> RuntimeSnapshot {
    let coordinator = state.coordinator.lock();
    RuntimeSnapshot {
        tabs: coordinator.connected_tabs.values().cloned().collect(),
        cooldown_remaining_seconds: coordinator.cooldown_remaining_seconds(),
        connected_hosts: coordinator.browser_ports.len() as u32,
        last_bridge_rejection: coordinator.last_bridge_rejection.clone(),
        last_host_disconnect_reason: coordinator.last_host_disconnect_reason.clone(),
        host_self_check: state.host_self_check.lock().clone(),
    }
}

/// The native-host auto-registration outcome computed on app startup.
#[tauri::command]
fn native_host_registration(state: State<'_, AppState>) -> native_host::AutoRegisterResult {
    state.native_host_registration.lock().clone()
}

#[tauri::command]
fn backup_settings_before_migration(app: tauri::AppHandle) -> Result<(), String> {
    let settings_path = app
        .path()
        .app_config_dir()
        .map_err(|error| error.to_string())?
        .join("settings.json");
    if settings_path.exists() {
        let backup_path = settings_path.with_file_name("settings.backup.json");
        fs::copy(&settings_path, backup_path).map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn request_dispatch(
    state: State<'_, AppState>,
    request: DispatchRequest,
) -> Result<DispatchOutcome, String> {
    request.validate().map_err(|error| error.to_string())?;
    {
        let coordinator = state.coordinator.lock();
        if coordinator.cooldown_remaining_seconds() > 0 {
            return Ok(DispatchOutcome::Rejected {
                reason: format!(
                    "全域冷卻仍剩 {} 秒",
                    coordinator.cooldown_remaining_seconds()
                ),
            });
        }
        if !coordinator
            .connected_tabs
            .contains_key(&target_key(&request.first_target))
            || !coordinator
                .connected_tabs
                .contains_key(&target_key(&request.second_target))
        {
            return Ok(DispatchOutcome::Rejected {
                reason: "目標未連線或已失效；請重新選擇兩個 Browser Tab".into(),
            });
        }
    }

    let first_prepare = prepare_target(
        &state.coordinator,
        &request.first_target,
        &request.request_id,
    )?;
    if !first_prepare.ready {
        return Ok(DispatchOutcome::Rejected {
            reason: prepare_failure_reason(&first_prepare),
        });
    }
    // Window activation is asynchronous on macOS/Windows; the previously
    // frontmost app can still receive the injected chord until the switch
    // commits, so wait for the target browser before posting.
    state.input_adapter
        .verify_foreground(&request.first_target.browser)
        .map_err(|error| error.to_string())?;
    if let Err(error) = state.input_adapter.send(&request.shortcut.chord) {
        return Ok(DispatchOutcome::Rejected {
            reason: error.to_string(),
        });
    }
    state.coordinator.lock().cooldown_started_at = Some(Instant::now());
    let first_attempt = DispatchAttempt {
        target: request.first_target.clone(),
        status: AttemptStatus::Attempted,
        detail: "已嘗試以 native input 注入快捷鍵；不保證送達。".into(),
    };

    let second_prepare = prepare_target(
        &state.coordinator,
        &request.second_target,
        &request.request_id,
    )?;
    if !second_prepare.ready {
        return Ok(DispatchOutcome::Partial {
            attempts: vec![
                first_attempt,
                DispatchAttempt {
                    target: request.second_target,
                    status: prepare_status(&second_prepare),
                    detail: prepare_failure_reason(&second_prepare),
                },
            ],
        });
    }
    state.input_adapter
        .verify_foreground(&request.second_target.browser)
        .map_err(|error| error.to_string())?;
    if let Err(error) = state.input_adapter.send(&request.shortcut.chord) {
        return Ok(DispatchOutcome::Partial {
            attempts: vec![
                first_attempt,
                DispatchAttempt {
                    target: request.second_target,
                    status: AttemptStatus::NotDelivered,
                    detail: error.to_string(),
                },
            ],
        });
    }

    Ok(DispatchOutcome::Attempted {
        attempts: vec![
            first_attempt,
            DispatchAttempt {
                target: request.second_target,
                status: AttemptStatus::Attempted,
                detail: "已嘗試以 native input 注入快捷鍵；不保證送達。".into(),
            },
        ],
    })
}

fn prepare_target(
    coordinator: &Arc<Mutex<DispatchCoordinator>>,
    target: &TabTarget,
    parent_request_id: &str,
) -> Result<bridge::PreparedResult, String> {
    let request_id = format!("{parent_request_id}:{}", target_key(target));
    let (sender, receiver) = std::sync::mpsc::channel();
    let port = {
        let mut coordinator = coordinator.lock();
        let port = coordinator
            .browser_ports
            .get(&bridge::connection_key_for_target(target))
            .cloned()
            .ok_or("目標的 browser bridge 已斷線")?;
        coordinator
            .pending_prepares
            .insert(request_id.clone(), sender);
        port
    };
    if port
        .send(json!({ "type": "prepare", "request_id": request_id, "target": target }))
        .is_err()
    {
        coordinator
            .lock()
            .pending_prepares
            .remove(&format!("{parent_request_id}:{}", target_key(target)));
        return Err("目標的 browser bridge 已斷線".into());
    }
    receiver
        .recv_timeout(PREPARE_TIMEOUT)
        .map_err(|_| "目標前景驗證逾時；發送已拒絕".into())
}

fn prepare_status(result: &bridge::PreparedResult) -> AttemptStatus {
    match result.code.as_deref() {
        Some("rejected_stale") => AttemptStatus::RejectedStale,
        Some("rejected_disconnected") => AttemptStatus::RejectedDisconnected,
        _ => AttemptStatus::FocusFailed,
    }
}

fn prepare_failure_reason(result: &bridge::PreparedResult) -> String {
    result
        .detail
        .clone()
        .unwrap_or_else(|| match result.code.as_deref() {
            Some("rejected_stale") => "目標已失效；請重新選擇。".into(),
            Some("rejected_disconnected") => "目標 browser bridge 已斷線。".into(),
            _ => "目標無法成為前景輸入目標。".into(),
        })
}

pub(crate) fn target_key(target: &TabTarget) -> String {
    format!(
        "{}:{}:{}:{}:{}",
        target.browser,
        target.browser_instance_id,
        target.session_nonce,
        target.window_id,
        target.tab_id
    )
}

fn main() {
    let coordinator = Arc::new(Mutex::new(DispatchCoordinator::default()));
    let bridge_coordinator = coordinator.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .manage(AppState {
            coordinator,
            input_adapter: PlatformInputAdapter,
            native_host_registration: Mutex::new(native_host::AutoRegisterResult::default()),
            host_self_check: Mutex::new(None),
        })
        .setup(move |app| {
            bridge::start(bridge_coordinator.clone()).map_err(std::io::Error::other)?;
            // Register the native host with every known browser channel before
            // the first extension handshake can arrive; failures stay
            // non-fatal and surface in the UI instead of blocking startup.
            let registration = match native_host::auto_register_native_hosts(app.handle()) {
                Ok(result) => result,
                Err(error) => {
                    eprintln!("banana-hand: native host auto-registration failed: {error}");
                    native_host::AutoRegisterResult::default()
                }
            };
            *app.state::<AppState>().native_host_registration.lock() = registration;
            *app.state::<AppState>().host_self_check.lock() = native_host::run_self_check();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            runtime_snapshot,
            request_dispatch,
            backup_settings_before_migration,
            native_host_registration,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Banana Hand desktop application");
}
