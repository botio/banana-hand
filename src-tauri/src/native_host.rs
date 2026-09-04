//! Registration of the desktop-app native messaging host with a browser.
//!
//! A browser launches the native host through a per-browser native-messaging
//! manifest that points at the host binary. On Linux/macOS the manifest is a
//! JSON file in a per-browser directory; on Windows the manifest file lives in
//! the user's data directory and an HKCU registry value points at it.
use std::fs;
use std::path::{Path, PathBuf};

use banana_hand_protocol::BrowserKind;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use tauri::Manager;

/// The native-messaging host name shared by the app, the host binary, and the
/// extension allowlist.
const HOST_NAME: &str = "dev.bananahand.dispatch_host";
const MANIFEST_DESCRIPTION: &str = "Banana Hand Native Messaging host";

/// Fixed Firefox (AMO) extension id, from `extensions/firefox/manifest.json`.
const FIREFOX_EXTENSION_ID: &str = "bridge@banana-hand.dev";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterNativeHostRequest {
    pub browser: BrowserKind,
    /// Absolute path to the native host binary. When omitted, defaults to a
    /// sibling of the running executable (the dev layout, where the app and the
    /// host are both built into `target/<profile>/`).
    #[serde(default)]
    pub host_path: Option<PathBuf>,
    /// Browser extension id. Required for the Chromium family (store-assigned);
    /// ignored for Firefox, whose id is fixed in the manifest.
    #[serde(default)]
    pub extension_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterNativeHostResult {
    /// Where the native-messaging manifest was written.
    pub manifest_path: PathBuf,
    /// Where the browser will look for the host (dir on Unix, registry key on
    /// Windows).
    pub registry_location: String,
    /// The host binary path the manifest points at.
    pub host_path: PathBuf,
    /// Whether that host binary currently exists on disk.
    pub host_exists: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum RegistrationError {
    #[error("Chromium 系 extension 的 id 由商店分配；請提供 extension id")]
    MissingExtensionId,
    #[error("native host 路徑必須是絕對路徑：{0}")]
    HostPathNotAbsolute(PathBuf),
    #[error("寫入 native-messaging manifest 失敗（{0}）：{1}")]
    WriteFailed(PathBuf, String),
    #[error("設定 Windows registry 失敗：{0}")]
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    RegistryFailed(String),
}

/// The file name of the native-messaging manifest.
pub fn manifest_file_name() -> String {
    format!("{HOST_NAME}.json")
}

/// Build the native-messaging manifest JSON for a browser.
pub fn build_manifest(browser: &BrowserKind, host_path: &Path, extension_id: &str) -> Value {
    let mut manifest = serde_json::json!({
        "name": HOST_NAME,
        "description": MANIFEST_DESCRIPTION,
        "path": host_path.to_string_lossy(),
        "type": "stdio",
    });
    match browser {
        BrowserKind::Firefox => {
            manifest["allowed_extensions"] =
                Value::Array(vec![Value::String(extension_id.to_owned())]);
            manifest
        }
        BrowserKind::Chrome => {
            manifest["allowed_origins"] = Value::Array(vec![Value::String(format!(
                "chrome-extension://{extension_id}/"
            ))]);
            manifest
        }
    }
}

/// The directory that holds native-messaging host manifests for a browser.
///
/// - Linux (XDG): `$HOME/.config/google-chrome/NativeMessagingHosts` for
///   Chrome; `$HOME/.mozilla/native-messaging-hosts` for Firefox.
/// - macOS: `~/Library/Google/Chrome/NativeMessagingHosts` for Chrome,
///   `~/Library/Application Support/Mozilla/NativeMessagingHosts` for Firefox.
/// - Windows: `%LOCALAPPDATA%\Banana Hand\native-host-manifests` (the browser
///   is pointed here through an HKCU registry value).
pub fn manifest_dir(browser: &BrowserKind, home: &Path, localappdata: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let _ = (browser, home);
        return localappdata
            .join("Banana Hand")
            .join("native-host-manifests");
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = localappdata;
        match browser {
            BrowserKind::Firefox => {
                #[cfg(target_os = "macos")]
                {
                    home.join("Library/Application Support/Mozilla/NativeMessagingHosts")
                }
                #[cfg(not(target_os = "macos"))]
                {
                    home.join(".mozilla/native-messaging-hosts")
                }
            }
            BrowserKind::Chrome => {
                #[cfg(target_os = "macos")]
                {
                    home.join("Library/Google/Chrome/NativeMessagingHosts")
                }
                #[cfg(not(target_os = "macos"))]
                {
                    home.join(".config/google-chrome/NativeMessagingHosts")
                }
            }
        }
    }
}

/// The full on-disk path of the native-messaging manifest for a browser.
pub fn manifest_file_path(browser: &BrowserKind, home: &Path, localappdata: &Path) -> PathBuf {
    manifest_dir(browser, home, localappdata).join(manifest_file_name())
}

/// A human-readable description of where the browser looks for the host.
fn describe_location(browser: &BrowserKind, home: &Path, localappdata: &Path) -> String {
    #[cfg(target_os = "windows")]
    {
        let _ = (home, localappdata);
        let subkey = match browser {
            BrowserKind::Firefox => r"Software\Mozilla\NativeMessagingHosts",
            BrowserKind::Chrome => r"Software\Google\Chrome\NativeMessagingHosts",
        };
        format!("HKCU\\{subkey}\\{HOST_NAME}（值指向 manifest 檔）")
    }
    #[cfg(not(target_os = "windows"))]
    {
        manifest_dir(browser, home, localappdata)
            .display()
            .to_string()
    }
}

/// Resolve the native host binary path: the explicit request value, or a
/// sibling of the running executable.
fn resolve_host_path(request: &RegisterNativeHostRequest) -> Result<PathBuf, RegistrationError> {
    match &request.host_path {
        Some(path) if path.is_absolute() => Ok(path.clone()),
        Some(path) => Err(RegistrationError::HostPathNotAbsolute(path.clone())),
        None => Ok(default_host_path()),
    }
}

fn default_host_path() -> PathBuf {
    let exe = std::env::current_exe()
        .map(|path| path)
        .unwrap_or_else(|_| PathBuf::from("target/debug"));
    let parent = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    let name = if cfg!(target_os = "windows") {
        "banana-hand-native-host.exe"
    } else {
        "banana-hand-native-host"
    };
    parent.join(name)
}

/// Resolve the extension id: the request value, or the fixed Firefox id.
fn resolve_extension_id(request: &RegisterNativeHostRequest) -> Result<String, RegistrationError> {
    match &request.extension_id {
        Some(id) if !id.trim().is_empty() => Ok(id.trim().to_owned()),
        Some(_) => Err(RegistrationError::MissingExtensionId),
        None => match request.browser {
            BrowserKind::Firefox => Ok(FIREFOX_EXTENSION_ID.to_owned()),
            BrowserKind::Chrome => Err(RegistrationError::MissingExtensionId),
        },
    }
}

/// Write the manifest to disk and (on Windows) point the registry at it.
///
/// `home` and `localappdata` are explicit so the pure write flow is unit
/// testable against a temp directory.
pub fn register_in(
    home: &Path,
    localappdata: &Path,
    request: &RegisterNativeHostRequest,
) -> Result<RegisterNativeHostResult, RegistrationError> {
    let host_path = resolve_host_path(request)?;
    let host_exists = host_path.exists();
    let extension_id = resolve_extension_id(request)?;
    let manifest_path = manifest_file_path(&request.browser, home, localappdata);

    if let Some(parent) = manifest_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            RegistrationError::WriteFailed(manifest_path.clone(), error.to_string())
        })?;
    }
    let manifest = build_manifest(&request.browser, &host_path, &extension_id);
    let encoded = serde_json::to_string_pretty(&manifest).map_err(|error| {
        RegistrationError::WriteFailed(manifest_path.clone(), error.to_string())
    })?;
    fs::write(&manifest_path, &encoded).map_err(|error| {
        RegistrationError::WriteFailed(manifest_path.clone(), error.to_string())
    })?;

    #[cfg(target_os = "windows")]
    set_windows_registry(&request.browser, &manifest_path)?;

    Ok(RegisterNativeHostResult {
        manifest_path,
        registry_location: describe_location(&request.browser, home, localappdata),
        host_path,
        host_exists,
    })
}

/// Tauri command entry point: resolves the home / LOCALAPPDATA and delegates to
/// [`register_in`].
#[tauri::command]
pub fn register_native_host(
    app: tauri::AppHandle,
    request: RegisterNativeHostRequest,
) -> Result<RegisterNativeHostResult, String> {
    let home = app.path().home_dir().map_err(|error| error.to_string())?;
    let localappdata_string = std::env::var("LOCALAPPDATA").unwrap_or_default();
    let localappdata = Path::new(&localappdata_string);
    register_in(&home, localappdata, &request).map_err(|error| error.to_string())
}

/// Write the HKCU registry value that points the browser at the manifest file.
#[cfg(target_os = "windows")]
fn set_windows_registry(
    browser: &BrowserKind,
    manifest_path: &Path,
) -> Result<(), RegistrationError> {
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_WOW64_64KEY, KEY_WRITE, REG_SZ, RegCloseKey, RegCreateKeyExW,
        RegSetValueExW,
    };

    let subkey = match browser {
        BrowserKind::Firefox => "Software\\Mozilla\\NativeMessagingHosts",
        BrowserKind::Chrome => "Software\\Google\\Chrome\\NativeMessagingHosts",
    };
    let subkey_wide = wide(subkey);
    let value_wide = wide(HOST_NAME);
    let data_wide = wide(&manifest_path.to_string_lossy());

    let mut hkey: HKEY = std::ptr::null_mut();
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            subkey_wide.as_ptr(),
            0,
            std::ptr::null(),
            0,
            KEY_WRITE | KEY_WOW64_64KEY,
            std::ptr::null(),
            &mut hkey,
            std::ptr::null_mut(),
        )
    };
    if status != 0 {
        return Err(RegistrationError::RegistryFailed(format!(
            "RegCreateKeyExW = {status}"
        )));
    }
    let status = unsafe {
        RegSetValueExW(
            hkey,
            value_wide.as_ptr(),
            0,
            REG_SZ,
            data_wide.as_ptr().cast(),
            (data_wide.len() * 2) as u32,
        )
    };
    unsafe {
        RegCloseKey(hkey);
    }
    if status != 0 {
        return Err(RegistrationError::RegistryFailed(format!(
            "RegSetValueExW = {status}"
        )));
    }
    Ok(())
}

/// Encode a string as a NUL-terminated wide string for a Windows API call.
#[cfg(target_os = "windows")]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use banana_hand_protocol::BrowserKind;

    #[test]
    fn firefox_manifest_uses_fixed_id_and_allowed_extensions() {
        let manifest = build_manifest(
            &BrowserKind::Firefox,
            Path::new("/opt/banana-hand/banana-hand-native-host"),
            FIREFOX_EXTENSION_ID,
        );
        assert_eq!(manifest["name"], HOST_NAME);
        assert_eq!(manifest["type"], "stdio");
        assert_eq!(manifest["path"], "/opt/banana-hand/banana-hand-native-host");
        assert_eq!(manifest["allowed_extensions"][0], "bridge@banana-hand.dev");
        assert!(manifest.get("allowed_origins").is_none());
    }

    #[test]
    fn chromium_manifest_uses_allowed_origins() {
        let manifest = build_manifest(&BrowserKind::Chrome, Path::new("/app/host.exe"), "abcd1234");
        assert_eq!(
            manifest["allowed_origins"][0],
            "chrome-extension://abcd1234/"
        );
        assert!(manifest.get("allowed_extensions").is_none());
    }

    #[test]
    fn linux_manifest_dirs_follow_xdg_layout() {
        let home = Path::new("/home/user");
        let localappdata = Path::new("");
        assert_eq!(
            manifest_dir(&BrowserKind::Firefox, home, localappdata),
            Path::new("/home/user/.mozilla/native-messaging-hosts")
        );
        assert_eq!(
            manifest_dir(&BrowserKind::Chrome, home, localappdata),
            Path::new("/home/user/.config/google-chrome/NativeMessagingHosts")
        );
        assert_eq!(
            manifest_file_path(&BrowserKind::Firefox, home, localappdata),
            Path::new(
                "/home/user/.mozilla/native-messaging-hosts/dev.bananahand.dispatch_host.json"
            )
        );
    }

    #[test]
    fn firefox_registration_writes_manifest_into_temp_home() {
        let temp = std::env::temp_dir().join(format!(
            "banana-hand-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|value| value.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&temp).expect("create temp home");
        // A stand-in native host binary inside the temp dir so host_exists is true.
        let host = temp.join("banana-hand-native-host");
        fs::write(&host, b"fake native host").expect("write fake host");
        let request = RegisterNativeHostRequest {
            browser: BrowserKind::Firefox,
            host_path: Some(host.clone()),
            extension_id: None,
        };
        let result = register_in(&temp, Path::new(""), &request).expect("register firefox");
        assert!(result.manifest_path.exists());
        assert!(result.host_exists);
        assert_eq!(result.host_path, host);
        assert!(result.registry_location.contains("native-messaging-hosts"));
        let written = fs::read_to_string(&result.manifest_path).expect("read written manifest");
        let parsed: Value = serde_json::from_str(&written).expect("valid json");
        assert_eq!(parsed["name"], HOST_NAME);
        assert_eq!(parsed["allowed_extensions"][0], "bridge@banana-hand.dev");
        assert_eq!(parsed["path"], host.to_string_lossy().to_string());
        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn chromium_without_extension_id_is_rejected() {
        let request = RegisterNativeHostRequest {
            browser: BrowserKind::Chrome,
            host_path: Some(PathBuf::from("/opt/banana-hand/banana-hand-native-host")),
            extension_id: None,
        };
        let error = register_in(Path::new("/home/user"), Path::new(""), &request)
            .expect_err("must reject chromium without id");
        assert!(matches!(error, RegistrationError::MissingExtensionId));
    }
}
