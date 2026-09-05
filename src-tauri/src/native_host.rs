//! Registration of the desktop-app native messaging host with browsers.
//!
//! A browser launches the native host through a per-browser native-messaging
//! manifest that points at the host binary. On Linux/macOS the manifest is a
//! JSON file in a per-browser directory; on Windows the Chrome family is
//! pointed at a manifest file through HKCU registry values and Firefox reads
//! the manifest from its standard directory.
//!
//! Each Chrome channel (stable, Beta, Canary) and Chromium look in their own
//! native-messaging directory, so auto-registration writes one manifest into
//! every known channel: the user's actual browser finds it regardless of
//! which one they run, and the app never needs to ask which browser they
use std::fs;
use std::path::{Path, PathBuf};

use banana_hand_protocol::BrowserKind;
use serde::Serialize;
use serde_json::Value;
use tauri::Manager;

/// The native-messaging host name shared by the app, the host binary, and the
/// extension allowlist.
const HOST_NAME: &str = "dev.bananahand.dispatch_host";
const MANIFEST_DESCRIPTION: &str = "Banana Hand Native Messaging host";

/// Fixed Firefox (AMO) extension id, from `extensions/firefox/manifest.json`.
const FIREFOX_EXTENSION_ID: &str = "bridge@banana-hand.dev";
/// Fixed Chromium extension id derived from `extensions/chromium/manifest.json`'s key.
const CHROMIUM_EXTENSION_ID: &str = "mooakjhlbkjfbmbmliklkmfmacnomlai";

/// A browser channel the native host can be registered with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostBrowser {
    /// Google Chrome stable channel.
    Chrome,
    /// Google Chrome Beta channel.
    ChromeBeta,
    /// Google Chrome Canary channel.
    ChromeCanary,
    /// Chromium (the non-Google build of Chrome).
    Chromium,
    /// Mozilla Firefox.
    Firefox,
}

impl HostBrowser {
    /// Lowercase wire name used in UI status text.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chrome => "chrome",
            Self::ChromeBeta => "chrome-beta",
            Self::ChromeCanary => "chrome-canary",
            Self::Chromium => "chromium",
            Self::Firefox => "firefox",
        }
    }

    /// The protocol-level browser kind the channel speaks.
    fn wire_kind(self) -> BrowserKind {
        match self {
            Self::Firefox => BrowserKind::Firefox,
            Self::Chrome | Self::ChromeBeta | Self::ChromeCanary | Self::Chromium => {
                BrowserKind::Chrome
            }
        }
    }
}

/// Every channel auto-registration writes a manifest into.
const ALL_BROWSERS: [HostBrowser; 5] = [
    HostBrowser::Chrome,
    HostBrowser::ChromeBeta,
    HostBrowser::ChromeCanary,
    HostBrowser::Chromium,
    HostBrowser::Firefox,
];

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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoRegisterEntry {
    /// Channel name (see [`HostBrowser::as_str`]).
    pub browser: String,
    /// Where the native-messaging manifest was written (or would have been,
    /// if the write failed).
    pub manifest_path: PathBuf,
    /// Where the browser will look for the host.
    pub registry_location: String,
    /// The host binary path the manifest points at.
    pub host_path: PathBuf,
    /// Whether that host binary currently exists on disk.
    pub host_exists: bool,
    /// Write/registry failure, when the entry could not be completed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoRegisterResult {
    pub entries: Vec<AutoRegisterEntry>,
}

#[derive(Debug, thiserror::Error)]
pub enum RegistrationError {
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
pub fn build_manifest(browser: &BrowserKind, host_path: &Path) -> Value {
    let mut manifest = serde_json::json!({
        "name": HOST_NAME,
        "description": MANIFEST_DESCRIPTION,
        "path": host_path.to_string_lossy(),
        "type": "stdio",
    });
    match browser {
        BrowserKind::Firefox => {
            manifest["allowed_extensions"] =
                Value::Array(vec![Value::String(FIREFOX_EXTENSION_ID.to_owned())]);
            manifest
        }
        BrowserKind::Chrome => {
            manifest["allowed_origins"] = Value::Array(vec![Value::String(format!(
                "chrome-extension://{CHROMIUM_EXTENSION_ID}/"
            ))]);
            manifest
        }
    }
}

/// The directory that holds native-messaging host manifests for a browser
/// channel.
///
/// - macOS: `~/Library/Google/Chrome{, Beta, Canary}/NativeMessagingHosts`
///   for the Chrome channels, `~/Library/Application Support/Chromium/…`
///   for Chromium, `~/Library/Application Support/Mozilla/NativeMessagingHosts`
///   for Firefox.
/// - Linux: `~/.config/{google-chrome,google-chrome-beta,google-chrome-canary,chromium}/
///   NativeMessagingHosts` and `~/.mozilla/native-messaging-hosts`.
/// - Windows: the Chrome channels share `%LOCALAPPDATA%\Banana Hand\native-host-manifests`
///   (pointed at by an HKCU registry value); Firefox reads
///   `%LOCALAPPDATA%\Mozilla\Firefox\NativeMessagingHosts` directly.
pub fn manifest_dir(browser: HostBrowser, home: &Path, localappdata: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let _ = home;
        match browser {
            HostBrowser::Firefox => localappdata.join("Mozilla/Firefox/NativeMessagingHosts"),
            _ => localappdata.join("Banana Hand").join("native-host-manifests"),
        }
    }
    #[cfg(target_os = "macos")]
    {
        let _ = localappdata;
        let sub = match browser {
            HostBrowser::Chrome => "Library/Google/Chrome/NativeMessagingHosts",
            HostBrowser::ChromeBeta => "Library/Google/Chrome Beta/NativeMessagingHosts",
            HostBrowser::ChromeCanary => "Library/Google/Chrome Canary/NativeMessagingHosts",
            HostBrowser::Chromium => "Library/Application Support/Chromium/NativeMessagingHosts",
            HostBrowser::Firefox => "Library/Application Support/Mozilla/NativeMessagingHosts",
        };
        home.join(sub)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = localappdata;
        let sub = match browser {
            HostBrowser::Chrome => ".config/google-chrome/NativeMessagingHosts",
            HostBrowser::ChromeBeta => ".config/google-chrome-beta/NativeMessagingHosts",
            HostBrowser::ChromeCanary => ".config/google-chrome-canary/NativeMessagingHosts",
            HostBrowser::Chromium => ".config/chromium/NativeMessagingHosts",
            HostBrowser::Firefox => ".mozilla/native-messaging-hosts",
        };
        home.join(sub)
    }
}

/// The full on-disk path of the native-messaging manifest for a channel.
pub fn manifest_file_path(browser: HostBrowser, home: &Path, localappdata: &Path) -> PathBuf {
    manifest_dir(browser, home, localappdata).join(manifest_file_name())
}

/// A human-readable description of where the browser looks for the host.
fn describe_location(browser: HostBrowser, home: &Path, localappdata: &Path) -> String {
    #[cfg(target_os = "windows")]
    {
        let _ = (home, localappdata);
        match browser {
            HostBrowser::Firefox => {
                format!(
                    "{}（檔案目錄）",
                    manifest_dir(browser, home, localappdata).display()
                )
            }
            _ => format!(
                "HKCU\\{}\\{}（值指向 manifest 檔）",
                windows_registry_subkey(browser),
                HOST_NAME
            ),
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        manifest_dir(browser, home, localappdata)
            .display()
            .to_string()
    }
}

/// The HKCU subkey a Chrome channel reads on Windows.
#[cfg(target_os = "windows")]
fn windows_registry_subkey(browser: HostBrowser) -> &'static str {
    match browser {
        HostBrowser::Chrome | HostBrowser::Chromium => "Software\\Google\\Chrome\\NativeMessagingHosts",
        HostBrowser::ChromeBeta => "Software\\Google\\ChromeBeta\\NativeMessagingHosts",
        HostBrowser::ChromeCanary => "Software\\Google\\ChromeCanary\\NativeMessagingHosts",
        HostBrowser::Firefox => unreachable!("Firefox uses a file directory on Windows"),
    }
}

/// Resolve the native host binary path: an explicit value, or a sibling of
/// the running executable (the sidecar layout).
pub fn default_host_path() -> PathBuf {
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

/// Write the manifest for one channel and (on Windows) point the registry at
/// it.
///
/// `home` and `localappdata` are explicit so the pure write flow is unit
/// testable against a temp directory.
pub fn register_in(
    browser: HostBrowser,
    home: &Path,
    localappdata: &Path,
    host_path: &Path,
) -> Result<RegisterNativeHostResult, RegistrationError> {
    let host_exists = host_path.exists();
    let manifest_path = manifest_file_path(browser, home, localappdata);

    if let Some(parent) = manifest_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            RegistrationError::WriteFailed(manifest_path.clone(), error.to_string())
        })?;
    }
    let manifest = build_manifest(&browser.wire_kind(), host_path);
    let encoded = serde_json::to_string_pretty(&manifest).map_err(|error| {
        RegistrationError::WriteFailed(manifest_path.clone(), error.to_string())
    })?;
    fs::write(&manifest_path, &encoded).map_err(|error| {
        RegistrationError::WriteFailed(manifest_path.clone(), error.to_string())
    })?;

    #[cfg(target_os = "windows")]
    set_windows_registry(browser, &manifest_path)?;

    Ok(RegisterNativeHostResult {
        manifest_path,
        registry_location: describe_location(browser, home, localappdata),
        host_path: host_path.to_path_buf(),
        host_exists,
    })
}

/// Write the native-messaging manifest into every known browser channel.
///
/// Per-channel failures are recorded on the entry instead of aborting the
/// batch, so one unreadable directory never blinds the rest.
pub fn auto_register(
    home: &Path,
    localappdata: &Path,
    host_path: Option<&Path>,
) -> AutoRegisterResult {
    let host = host_path.map(Path::to_path_buf).unwrap_or_else(default_host_path);
    let host_exists = host.exists();
    let mut result = AutoRegisterResult::default();
    for browser in ALL_BROWSERS {
        match register_in(browser, home, localappdata, &host) {
            Ok(entry) => result.entries.push(AutoRegisterEntry {
                browser: browser.as_str().to_owned(),
                manifest_path: entry.manifest_path,
                registry_location: entry.registry_location,
                host_path: entry.host_path,
                host_exists,
                error: None,
            }),
            Err(error) => result.entries.push(AutoRegisterEntry {
                browser: browser.as_str().to_owned(),
                manifest_path: manifest_file_path(browser, home, localappdata),
                registry_location: describe_location(browser, home, localappdata),
                host_path: host.clone(),
                host_exists,
                error: Some(error.to_string()),
            }),
        }
    }
    result
}

/// Auto-registers the native host with every known browser channel using the
/// default (sidecar) host path. Called once on app startup.
pub fn auto_register_native_hosts(app: &tauri::AppHandle) -> Result<AutoRegisterResult, String> {
    let home = app
        .path()
        .home_dir()
        .map_err(|error| error.to_string())?;
    let localappdata_string = std::env::var("LOCALAPPDATA").unwrap_or_default();
    Ok(auto_register(&home, Path::new(&localappdata_string), None))
}

/// Write the HKCU registry value that points a Chrome channel at the manifest
/// file.
#[cfg(target_os = "windows")]
fn set_windows_registry(
    browser: HostBrowser,
    manifest_path: &Path,
) -> Result<(), RegistrationError> {
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_WOW64_64KEY, KEY_WRITE, REG_SZ, RegCloseKey, RegCreateKeyExW,
        RegSetValueExW,
    };

    let subkey = windows_registry_subkey(browser);
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

    #[test]
    fn firefox_manifest_uses_fixed_id_and_allowed_extensions() {
        let manifest = build_manifest(
            &BrowserKind::Firefox,
            Path::new("/opt/banana-hand/banana-hand-native-host"),
        );
        assert_eq!(manifest["name"], HOST_NAME);
        assert_eq!(manifest["type"], "stdio");
        assert_eq!(manifest["path"], "/opt/banana-hand/banana-hand-native-host");
        assert_eq!(manifest["allowed_extensions"][0], FIREFOX_EXTENSION_ID);
        assert!(manifest.get("allowed_origins").is_none());
    }

    #[test]
    fn chromium_manifest_uses_fixed_extension_id() {
        let manifest = build_manifest(&BrowserKind::Chrome, Path::new("/app/host.exe"));
        assert_eq!(
            manifest["allowed_origins"][0],
            format!("chrome-extension://{CHROMIUM_EXTENSION_ID}/")
        );
        assert!(manifest.get("allowed_extensions").is_none());
    }

    #[test]
    fn manifest_dirs_cover_every_chrome_channel() {
        let home = Path::new("/home/user");
        let localappdata = Path::new("");
        // The exact directories differ per OS; assert the channel layout
        // (distinct dir per channel, all under the right browser root).
        let chrome = manifest_dir(HostBrowser::Chrome, home, localappdata);
        let beta = manifest_dir(HostBrowser::ChromeBeta, home, localappdata);
        let canary = manifest_dir(HostBrowser::ChromeCanary, home, localappdata);
        let chromium = manifest_dir(HostBrowser::Chromium, home, localappdata);
        let firefox = manifest_dir(HostBrowser::Firefox, home, localappdata);
        assert_ne!(chrome, beta);
        assert_ne!(chrome, canary);
        assert_ne!(chrome, chromium);
        for dir in [&chrome, &beta, &canary, &chromium] {
            assert!(
                dir.to_string_lossy().contains("NativeMessagingHosts"),
                "chrome-family dir must end in NativeMessagingHosts: {dir:?}"
            );
        }
        assert_ne!(&firefox, &chrome);
        for browser in ALL_BROWSERS {
            let file = manifest_file_path(browser, home, localappdata);
            assert_eq!(
                file.file_name().unwrap().to_string_lossy(),
                manifest_file_name()
            );
        }
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
        let result =
            register_in(HostBrowser::Firefox, &temp, Path::new(""), &host).expect("register firefox");
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
    fn auto_register_writes_one_manifest_per_channel() {
        let temp = std::env::temp_dir().join(format!(
            "banana-hand-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|value| value.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&temp).expect("create temp home");
        let host = temp.join("banana-hand-native-host");
        fs::write(&host, b"fake native host").expect("write fake host");

        let result = auto_register(&temp, Path::new(""), Some(&host));
        assert_eq!(result.entries.len(), ALL_BROWSERS.len());
        assert!(
            result.entries.iter().all(|entry| entry.error.is_none()),
            "all entries should succeed in a writable temp home: {:?}",
            result.entries.iter().map(|entry| &entry.error).collect::<Vec<_>>()
        );
        // Every entry wrote a distinct manifest with the right allowlist.
        let mut seen = std::collections::HashSet::new();
        for entry in &result.entries {
            assert!(seen.insert(entry.manifest_path.clone()));
            let parsed: Value =
                serde_json::from_str(&fs::read_to_string(&entry.manifest_path).unwrap()).unwrap();
            assert_eq!(parsed["name"], HOST_NAME);
            match HostBrowser::as_str_inverse(&entry.browser) {
                Some(HostBrowser::Firefox) => {
                    assert_eq!(parsed["allowed_extensions"][0], FIREFOX_EXTENSION_ID);
                    assert!(parsed.get("allowed_origins").is_none());
                }
                _ => {
                    assert_eq!(
                        parsed["allowed_origins"][0],
                        format!("chrome-extension://{CHROMIUM_EXTENSION_ID}/")
                    );
                    assert!(parsed.get("allowed_extensions").is_none());
                }
            }
        }
        let _ = fs::remove_dir_all(&temp);
    }

    impl HostBrowser {
        fn as_str_inverse(name: &str) -> Option<HostBrowser> {
            ALL_BROWSERS
                .iter()
                .copied()
                .find(|browser| browser.as_str() == name)
        }
    }
}
