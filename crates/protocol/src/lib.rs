use std::{fmt, path::PathBuf, str::FromStr};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const PROTOCOL_MAJOR: u16 = 1;

/// Returns the private runtime directory shared by the desktop App and its
/// browser-launched native host. The directory itself must be mode 0700.
#[cfg(target_os = "linux")]
pub fn local_runtime_directory() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .map(|directory| directory.join("banana-hand"))
}

/// macOS has no XDG runtime directory. The per-user cache directory supplies
/// an owner-scoped location for the Unix socket and ephemeral capability token.
#[cfg(target_os = "macos")]
pub fn local_runtime_directory() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from).map(|home| {
        home.join("Library")
            .join("Caches")
            .join("Banana Hand")
            .join("runtime")
    })
}

/// Windows uses the per-user LOCALAPPDATA directory. A named pipe (not a Unix
/// socket) carries the bridge; its name and capability token live in the config.
#[cfg(target_os = "windows")]
pub fn local_runtime_directory() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|directory| directory.join("Banana Hand").join("runtime"))
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub fn local_runtime_directory() -> Option<PathBuf> {
    None
}

/// The per-user bridge configuration written by the desktop App and read by the
/// browser-launched native host. Exactly one of `socket_path` (Unix) or
/// `pipe_name` (Windows) is set for a given platform build.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeHostBridgeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub socket_path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pipe_name: Option<String>,
    pub capability_token: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum BrowserKind {
    Chrome,
    Firefox,
}

impl fmt::Display for BrowserKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Chrome => "Chrome",
            Self::Firefox => "Firefox",
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct TabTarget {
    pub browser: BrowserKind,
    pub browser_instance_id: String,
    pub session_nonce: String,
    pub window_id: u32,
    pub tab_id: u32,
    pub generation: u64,
}

impl TabTarget {
    pub fn is_same_target_as(&self, other: &Self) -> bool {
        self.browser == other.browser
            && self.browser_instance_id == other.browser_instance_id
            && self.session_nonce == other.session_nonce
            && self.window_id == other.window_id
            && self.tab_id == other.tab_id
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct TabMetadata {
    pub target: TabTarget,
    pub title: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Hash, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Meta,
}

impl Modifier {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "Ctrl" => Some(Self::Ctrl),
            "Alt" => Some(Self::Alt),
            "Shift" => Some(Self::Shift),
            "Meta" => Some(Self::Meta),
            _ => None,
        }
    }
}

impl fmt::Display for Modifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Ctrl => "Ctrl",
            Self::Alt => "Alt",
            Self::Shift => "Shift",
            Self::Meta => "Meta",
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Key {
    Character(char),
    Function(u8),
    Escape,
    Enter,
    Tab,
    Space,
}

impl fmt::Display for Key {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Character(value) => write!(formatter, "{value}"),
            Self::Function(value) => write!(formatter, "F{value}"),
            Self::Escape => formatter.write_str("Esc"),
            Self::Enter => formatter.write_str("Enter"),
            Self::Tab => formatter.write_str("Tab"),
            Self::Space => formatter.write_str("Space"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct ShortcutChord {
    pub modifiers: Vec<Modifier>,
    pub key: Key,
}

impl fmt::Display for ShortcutChord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for modifier in &self.modifiers {
            write!(formatter, "{modifier}+")?;
        }
        write!(formatter, "{}", self.key)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ShortcutParseError {
    #[error("快捷鍵不可為空白")]
    Empty,
    #[error("快捷鍵不可包含空白片段")]
    EmptySegment,
    #[error("修飾鍵必須在主要按鍵前")]
    ModifierAfterKey,
    #[error("修飾鍵重複：{0}")]
    DuplicateModifier(Modifier),
    #[error("不支援的主要按鍵：{0}")]
    UnsupportedKey(String),
}

impl FromStr for ShortcutChord {
    type Err = ShortcutParseError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        if raw.trim().is_empty() {
            return Err(ShortcutParseError::Empty);
        }

        let segments = raw.split('+').map(str::trim).collect::<Vec<_>>();
        if segments.iter().any(|segment| segment.is_empty()) {
            return Err(ShortcutParseError::EmptySegment);
        }

        let mut modifiers = Vec::new();
        let mut key = None;
        for segment in segments {
            if let Some(modifier) = Modifier::parse(segment) {
                if key.is_some() {
                    return Err(ShortcutParseError::ModifierAfterKey);
                }
                if modifiers.contains(&modifier) {
                    return Err(ShortcutParseError::DuplicateModifier(modifier));
                }
                modifiers.push(modifier);
                continue;
            }

            if key.is_some() {
                return Err(ShortcutParseError::UnsupportedKey(segment.to_owned()));
            }
            key = Some(parse_key(segment)?);
        }

        Ok(Self {
            modifiers,
            key: key.ok_or(ShortcutParseError::Empty)?,
        })
    }
}

fn parse_key(raw: &str) -> Result<Key, ShortcutParseError> {
    match raw {
        "Esc" => Ok(Key::Escape),
        "Enter" => Ok(Key::Enter),
        "Tab" => Ok(Key::Tab),
        "Space" => Ok(Key::Space),
        _ if raw.starts_with('F') => raw[1..]
            .parse::<u8>()
            .ok()
            .filter(|number| (1..=24).contains(number))
            .map(Key::Function)
            .ok_or_else(|| ShortcutParseError::UnsupportedKey(raw.to_owned())),
        _ if raw.chars().count() == 1
            && raw
                .chars()
                .all(|character| character.is_ascii_alphanumeric()) =>
        {
            Ok(Key::Character(
                raw.chars()
                    .next()
                    .expect("one character")
                    .to_ascii_uppercase(),
            ))
        }
        _ => Err(ShortcutParseError::UnsupportedKey(raw.to_owned())),
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Shortcut {
    pub id: String,
    pub name: String,
    pub chord: ShortcutChord,
    pub order: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct DispatchRequest {
    pub request_id: String,
    pub shortcut: Shortcut,
    pub first_target: TabTarget,
    pub second_target: TabTarget,
}

impl DispatchRequest {
    pub fn validate(&self) -> Result<(), DispatchValidationError> {
        if self.first_target.is_same_target_as(&self.second_target) {
            return Err(DispatchValidationError::DuplicateTarget);
        }
        if self.shortcut.name.trim().is_empty() {
            return Err(DispatchValidationError::EmptyShortcutName);
        }
        Ok(())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DispatchValidationError {
    #[error("兩個目標必須是不同的 Browser Tab")]
    DuplicateTarget,
    #[error("快捷鍵名稱不可為空白")]
    EmptyShortcutName,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AttemptStatus {
    Attempted,
    RejectedStale,
    RejectedDisconnected,
    PermissionDenied,
    FocusFailed,
    NotDelivered,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct DispatchAttempt {
    pub target: TabTarget,
    pub status: AttemptStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DispatchOutcome {
    Rejected { reason: String },
    Partial { attempts: Vec<DispatchAttempt> },
    Attempted { attempts: Vec<DispatchAttempt> },
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct NativeHostMessage {
    pub request_id: String,
    pub message: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct HostBridgeRequest {
    pub capability_token: String,
    pub message: NativeHostMessage,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct HostBridgeResponse {
    pub response: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_shortcut_chords() {
        let chord = "Ctrl+Shift+k".parse::<ShortcutChord>().unwrap();
        assert_eq!(chord.to_string(), "Ctrl+Shift+K");
    }

    #[test]
    fn rejects_duplicate_targets() {
        let target = TabTarget {
            browser: BrowserKind::Chrome,
            browser_instance_id: "profile-a".into(),
            session_nonce: "session-a".into(),
            window_id: 1,
            tab_id: 2,
            generation: 1,
        };
        let request = DispatchRequest {
            request_id: "request-a".into(),
            shortcut: Shortcut {
                id: "shortcut-a".into(),
                name: "確認".into(),
                chord: "F8".parse().unwrap(),
                order: 0,
            },
            first_target: target.clone(),
            second_target: target,
        };

        assert_eq!(
            request.validate(),
            Err(DispatchValidationError::DuplicateTarget)
        );
    }
    #[test]
    #[cfg(target_os = "linux")]
    fn linux_runtime_directory_uses_xdg_path() {
        unsafe { std::env::set_var("XDG_RUNTIME_DIR", "/run/user/1000") };
        assert_eq!(
            local_runtime_directory(),
            Some(std::path::PathBuf::from("/run/user/1000/banana-hand"))
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn macos_runtime_directory_uses_owner_cache_path() {
        unsafe { std::env::set_var("HOME", "/Users/tester") };
        assert_eq!(
            local_runtime_directory(),
            Some(std::path::PathBuf::from(
                "/Users/tester/Library/Caches/Banana Hand/runtime"
            ))
        );
    }
}
