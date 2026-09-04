use banana_hand_protocol::{Key, Modifier, ShortcutChord};
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum InputError {
    #[error("目前桌面工作階段不是受支援的 X11、Wayland portal、Windows 或 macOS 工作階段")]
    UnsupportedSession,
    #[error("Wayland Remote Desktop portal 尚未授權或尚無可用 backend；發送已被權限閘門拒絕")]
    PortalPermissionRequired,
    #[cfg(target_os = "macos")]
    #[error("macOS Accessibility 尚未授權；發送已被權限閘門拒絕")]
    AccessibilityPermissionRequired,
    #[cfg(target_os = "macos")]
    #[error("macOS 無法建立 CGEvent")]
    MacEventUnavailable,
    #[cfg(target_os = "windows")]
    #[error("Windows SendInput 未接受完整快捷鍵事件串流")]
    WindowsInjectionFailed,
    #[error("X11 display 無法開啟")]
    X11DisplayUnavailable,
    #[error("X11 server 未提供 XTEST extension")]
    X11TestUnavailable,
    #[error("快捷鍵無法映射為目前 keyboard layout 的 keycode")]
    KeycodeUnavailable,
    #[error("XTEST 拒絕輸入事件")]
    X11InjectionFailed,
}

pub(crate) trait InputAdapter {
    fn send(&self, chord: &ShortcutChord) -> Result<(), InputError>;
}

pub(crate) struct PlatformInputAdapter;

impl InputAdapter for PlatformInputAdapter {
    fn send(&self, chord: &ShortcutChord) -> Result<(), InputError> {
        send_for_current_platform(chord)
    }
}

#[cfg(target_os = "linux")]
fn send_for_current_platform(chord: &ShortcutChord) -> Result<(), InputError> {
    match std::env::var("XDG_SESSION_TYPE").as_deref() {
        Ok("x11") => send_x11(chord),
        Ok("wayland") => Err(InputError::PortalPermissionRequired),
        _ => Err(InputError::UnsupportedSession),
    }
}

#[cfg(target_os = "windows")]
fn send_for_current_platform(chord: &ShortcutChord) -> Result<(), InputError> {
    send_windows(chord)
}

#[cfg(target_os = "macos")]
fn send_for_current_platform(chord: &ShortcutChord) -> Result<(), InputError> {
    send_macos(chord)
}

#[cfg(all(
    not(target_os = "linux"),
    not(target_os = "windows"),
    not(target_os = "macos")
))]
fn send_for_current_platform(_chord: &ShortcutChord) -> Result<(), InputError> {
    Err(InputError::UnsupportedSession)
}

#[cfg(target_os = "linux")]
fn send_x11(chord: &ShortcutChord) -> Result<(), InputError> {
    use std::ptr;
    use x11::{xlib, xtest};

    // Xlib/XTEST own the Display pointer. Every return path below closes it.
    unsafe {
        let display = xlib::XOpenDisplay(ptr::null());
        if display.is_null() {
            return Err(InputError::X11DisplayUnavailable);
        }

        let mut event_base = 0;
        let mut error_base = 0;
        let mut major_version = 0;
        let mut minor_version = 0;
        if xtest::XTestQueryExtension(
            display,
            &mut event_base,
            &mut error_base,
            &mut major_version,
            &mut minor_version,
        ) == 0
        {
            xlib::XCloseDisplay(display);
            return Err(InputError::X11TestUnavailable);
        }

        let modifier_codes = chord
            .modifiers
            .iter()
            .map(|modifier| keycode(display, modifier_keysym(*modifier)))
            .collect::<Result<Vec<_>, _>>();
        let main_code = keycode(display, key_keysym(&chord.key));
        let result = match (modifier_codes, main_code) {
            (Ok(modifier_codes), Ok(main_code)) => {
                let mut accepted = true;
                for modifier_code in &modifier_codes {
                    accepted &= xtest::XTestFakeKeyEvent(display, *modifier_code, 1, 0) != 0;
                }
                accepted &= xtest::XTestFakeKeyEvent(display, main_code, 1, 0) != 0;
                accepted &= xtest::XTestFakeKeyEvent(display, main_code, 0, 0) != 0;
                for modifier_code in modifier_codes.iter().rev() {
                    accepted &= xtest::XTestFakeKeyEvent(display, *modifier_code, 0, 0) != 0;
                }
                xlib::XFlush(display);
                accepted.then_some(()).ok_or(InputError::X11InjectionFailed)
            }
            (Err(error), _) | (_, Err(error)) => Err(error),
        };
        xlib::XCloseDisplay(display);
        result
    }
}

#[cfg(target_os = "linux")]
fn keycode(display: *mut x11::xlib::Display, keysym: u64) -> Result<u32, InputError> {
    // Display is opened immediately before this call and remains valid until
    // the enclosing XTEST sequence finishes.
    let code = unsafe { x11::xlib::XKeysymToKeycode(display, keysym as _) };
    (code != 0)
        .then_some(code as u32)
        .ok_or(InputError::KeycodeUnavailable)
}

#[cfg(target_os = "linux")]
fn modifier_keysym(modifier: Modifier) -> u64 {
    use x11::keysym;

    match modifier {
        Modifier::Ctrl => keysym::XK_Control_L as u64,
        Modifier::Alt => keysym::XK_Alt_L as u64,
        Modifier::Shift => keysym::XK_Shift_L as u64,
        Modifier::Meta => keysym::XK_Meta_L as u64,
    }
}

#[cfg(target_os = "linux")]
fn key_keysym(key: &Key) -> u64 {
    use x11::keysym;

    match key {
        Key::Character(character) => character.to_ascii_lowercase() as u64,
        Key::Function(number) => (keysym::XK_F1 + (*number as u32 - 1)) as u64,
        Key::Escape => keysym::XK_Escape as u64,
        Key::Enter => keysym::XK_Return as u64,
        Key::Tab => keysym::XK_Tab as u64,
        Key::Space => keysym::XK_space as u64,
    }
}

#[cfg(target_os = "windows")]
fn send_windows(chord: &ShortcutChord) -> Result<(), InputError> {
    use std::mem::size_of;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{INPUT, KEYEVENTF_KEYUP, SendInput};

    let mut keys = chord
        .modifiers
        .iter()
        .map(|modifier| windows_modifier_key(*modifier))
        .collect::<Vec<_>>();
    keys.push(windows_key(&chord.key));
    let mut inputs = Vec::with_capacity(keys.len() * 2);
    for key in &keys {
        inputs.push(keyboard_input(*key, 0));
    }
    for key in keys.iter().rev() {
        inputs.push(keyboard_input(*key, KEYEVENTF_KEYUP));
    }
    let accepted = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<INPUT>() as i32,
        )
    };
    (accepted as usize == inputs.len())
        .then_some(())
        .ok_or(InputError::WindowsInjectionFailed)
}

#[cfg(target_os = "windows")]
fn keyboard_input(key: u16, flags: u32) -> windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
    };

    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

#[cfg(target_os = "windows")]
fn windows_modifier_key(modifier: Modifier) -> u16 {
    match modifier {
        Modifier::Ctrl => 0x11,
        Modifier::Alt => 0x12,
        Modifier::Shift => 0x10,
        Modifier::Meta => 0x5B,
    }
}

#[cfg(target_os = "windows")]
fn windows_key(key: &Key) -> u16 {
    match key {
        Key::Character(character) => character.to_ascii_uppercase() as u16,
        Key::Function(number) => 0x70 + (*number as u16 - 1),
        Key::Escape => 0x1B,
        Key::Enter => 0x0D,
        Key::Tab => 0x09,
        Key::Space => 0x20,
    }
}

#[cfg(target_os = "macos")]
fn send_macos(chord: &ShortcutChord) -> Result<(), InputError> {
    use core_graphics::{
        event::{CGEvent, CGEventFlags, CGEventTapLocation},
        event_source::{CGEventSource, CGEventSourceStateID},
    };

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }
    if !unsafe { AXIsProcessTrusted() } {
        return Err(InputError::AccessibilityPermissionRequired);
    }
    let flags = chord
        .modifiers
        .iter()
        .fold(CGEventFlags::empty(), |flags, modifier| {
            flags
                | match modifier {
                    Modifier::Ctrl => CGEventFlags::CGEventFlagControl,
                    Modifier::Alt => CGEventFlags::CGEventFlagAlternate,
                    Modifier::Shift => CGEventFlags::CGEventFlagShift,
                    Modifier::Meta => CGEventFlags::CGEventFlagCommand,
                }
        });
    let keycode = macos_keycode(&chord.key);
    let down = CGEvent::new_keyboard_event(
        CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|_| InputError::MacEventUnavailable)?,
        keycode,
        true,
    )
    .map_err(|_| InputError::MacEventUnavailable)?;
    down.set_flags(flags);
    down.post(CGEventTapLocation::HID);
    let up = CGEvent::new_keyboard_event(
        CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|_| InputError::MacEventUnavailable)?,
        keycode,
        false,
    )
    .map_err(|_| InputError::MacEventUnavailable)?;
    up.set_flags(flags);
    up.post(CGEventTapLocation::HID);
    Ok(())
}

#[cfg(target_os = "macos")]
fn macos_keycode(key: &Key) -> u16 {
    match key {
        Key::Character(character) => match character.to_ascii_uppercase() {
            'A' => 0,
            'B' => 11,
            'C' => 8,
            'D' => 2,
            'E' => 14,
            'F' => 3,
            'G' => 5,
            'H' => 4,
            'I' => 34,
            'J' => 38,
            'K' => 40,
            'L' => 37,
            'M' => 46,
            'N' => 45,
            'O' => 31,
            'P' => 35,
            'Q' => 12,
            'R' => 15,
            'S' => 1,
            'T' => 17,
            'U' => 32,
            'V' => 9,
            'W' => 13,
            'X' => 7,
            'Y' => 16,
            'Z' => 6,
            '0' => 29,
            '1' => 18,
            '2' => 19,
            '3' => 20,
            '4' => 21,
            '5' => 23,
            '6' => 22,
            '7' => 26,
            '8' => 28,
            '9' => 25,
            _ => 0,
        },
        Key::Function(number) => match number {
            1 => 122,
            2 => 120,
            3 => 99,
            4 => 118,
            5 => 96,
            6 => 97,
            7 => 98,
            8 => 100,
            9 => 101,
            10 => 109,
            11 => 103,
            12 => 111,
            13 => 105,
            14 => 107,
            15 => 113,
            16 => 106,
            17 => 64,
            18 => 79,
            19 => 80,
            20 => 90,
            21 => 91,
            22 => 92,
            23 => 93,
            24 => 94,
            _ => 0,
        },
        Key::Escape => 53,
        Key::Enter => 36,
        Key::Tab => 48,
        Key::Space => 49,
    }
}
