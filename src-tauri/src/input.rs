use banana_hand_protocol::{BrowserKind, Key, Modifier, ShortcutChord};
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
    #[error("目標視窗沒有成為前景（目前前景：{actual}）；發送已拒絕，請再試一次")]
    ForegroundNotTarget {
        actual: String,
    },
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
    /// Wait until the target browser's window is actually frontmost, so the
    /// injected chord cannot land in whichever app was foreground a moment
    /// earlier. Default: the platform cannot verify foreground (Linux), no-op.
    fn verify_foreground(&self, _browser: &BrowserKind) -> Result<(), InputError> {
        Ok(())
    }
    fn send(&self, chord: &ShortcutChord) -> Result<(), InputError>;
}

pub(crate) struct PlatformInputAdapter;

#[cfg(target_os = "macos")]
impl InputAdapter for PlatformInputAdapter {
    fn verify_foreground(&self, browser: &BrowserKind) -> Result<(), InputError> {
        verify_macos_foreground(browser)
    }
    fn send(&self, chord: &ShortcutChord) -> Result<(), InputError> {
        send_macos(chord)
    }
}

#[cfg(target_os = "windows")]
impl InputAdapter for PlatformInputAdapter {
    fn verify_foreground(&self, browser: &BrowserKind) -> Result<(), InputError> {
        verify_windows_foreground(browser)
    }
    fn send(&self, chord: &ShortcutChord) -> Result<(), InputError> {
        send_windows(chord)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
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
    use std::ffi::c_void;

    use core_graphics::{
        event::{CGEvent, CGEventFlags, CGEventTapLocation},
        event_source::{CGEventSource, CGEventSourceStateID},
    };

    #[link(name = "ApplicationServices", kind = "framework")]
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
        static kCFBooleanTrue: *const c_void;
        fn CFStringCreateWithCString(
            alloc: *const c_void,
            string: *const u8,
            encoding: u32,
        ) -> *const c_void;
        fn CFDictionaryCreate(
            alloc: *const c_void,
            keys: *const *const c_void,
            values: *const *const c_void,
            count: isize,
        ) -> *const c_void;
        fn CFRelease(value: *const c_void);
    }
    // The options variant pops the system dialog (with a deep link into
    // System Settings) when the grant is missing, instead of failing with a
    // message the user has to translate into settings clicks.
    unsafe {
        let key = CFStringCreateWithCString(
            std::ptr::null(),
            b"kAXTrustedCheckOptionPrompt\0".as_ptr(),
            0x0800_0100,
        );
        let prompt: *const c_void = kCFBooleanTrue;
        let keys = [key];
        let values = [prompt];
        let options = CFDictionaryCreate(std::ptr::null(), keys.as_ptr(), values.as_ptr(), 1);
        let trusted = AXIsProcessTrustedWithOptions(options);
        CFRelease(options);
        CFRelease(key);
        if !trusted {
            return Err(InputError::AccessibilityPermissionRequired);
        }
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
fn verify_macos_foreground(browser: &BrowserKind) -> Result<(), InputError> {
    let expected: &[&str] = match browser {
        BrowserKind::Chrome => &[
            "Google Chrome",
            "Google Chrome Beta",
            "Google Chrome Canary",
            "Chromium",
        ],
        BrowserKind::Firefox => &["Firefox"],
    };
    // Window activation is asynchronous on macOS: until the window server
    // commits the switch, the previously frontmost app still receives
    // injected HID events.
    for _ in 0..20 {
        if let Some(owner) = frontmost_window_owner() {
            if expected.iter().any(|name| *name == owner) {
                std::thread::sleep(std::time::Duration::from_millis(100));
                return Ok(());
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(75));
    }
    Err(InputError::ForegroundNotTarget {
        actual: frontmost_window_owner().unwrap_or_default(),
    })
}

#[cfg(target_os = "macos")]
fn frontmost_window_owner() -> Option<String> {
    use std::ffi::c_void;

    #[link(name = "CoreFoundation", kind = "framework")]
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGWindowListCopyWindowInfo(option: u32, relative_to_window: u32) -> *const c_void;
        fn CFArrayGetCount(array: *const c_void) -> isize;
        fn CFArrayGetValueAtIndex(array: *const c_void, index: isize) -> *const c_void;
        fn CFDictionaryGetValue(dict: *const c_void, key: *const c_void) -> *const c_void;
        fn CFStringCreateWithCString(
            alloc: *const c_void,
            string: *const u8,
            encoding: u32,
        ) -> *const c_void;
        fn CFNumberGetValue(number: *const c_void, number_type: u32, value: *mut i64) -> bool;
        fn CFGetTypeID(value: *const c_void) -> u32;
        fn CFNumberGetTypeID() -> u32;
        fn CFStringGetTypeID() -> u32;
        fn CFRelease(value: *const c_void);
    }

    const UTF8: u32 = 0x0800_0100;
    const SINT64: u32 = 4; // kCFNumberSInt64Type (NOT 6: that is Float64)
    const ON_SCREEN: u32 = 1; // kCGWindowListOptionOnScreenOnly
    const EXCLUDE_DESKTOP: u32 = 2; // kCGWindowListExcludeDesktopElements

    unsafe {
        let array = CGWindowListCopyWindowInfo(ON_SCREEN | EXCLUDE_DESKTOP, 0);
        if array.is_null() {
            return None;
        }
        let layer_key =
            CFStringCreateWithCString(std::ptr::null(), b"kCGWindowLayer\0".as_ptr(), UTF8);
        let owner_key =
            CFStringCreateWithCString(std::ptr::null(), b"kCGWindowOwnerName\0".as_ptr(), UTF8);
        let mut found = None;
        for index in 0..CFArrayGetCount(array) {
            let window = CFArrayGetValueAtIndex(array, index);
            let layer = CFDictionaryGetValue(window, layer_key);
            if layer.is_null() || CFGetTypeID(layer) != CFNumberGetTypeID() {
                continue;
            }
            let mut layer_value: i64 = -1;
            if !CFNumberGetValue(layer, SINT64, &mut layer_value) || layer_value != 0 {
                continue;
            }
            let owner = CFDictionaryGetValue(window, owner_key);
            if !owner.is_null() && CFGetTypeID(owner) == CFStringGetTypeID() {
                found = cf_string_to_rust(owner);
                break;
            }
        }
        CFRelease(layer_key);
        CFRelease(owner_key);
        CFRelease(array);
        found
    }
}

#[cfg(target_os = "macos")]
fn cf_string_to_rust(value: *const std::ffi::c_void) -> Option<String> {
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFStringGetLength(string: *const std::ffi::c_void) -> isize;
        fn CFStringGetCString(
            string: *const std::ffi::c_void,
            buffer: *mut u8,
            size: isize,
            encoding: u32,
        ) -> u32;
        fn CFStringGetBytes(
            string: *const std::ffi::c_void,
            range: CFRange,
            encoding: u32,
            loss_byte: u16,
            external: u8,
            buffer: *mut u8,
            max_len: isize,
            used: *mut isize,
        ) -> isize;
    }

    // CFRange from CFBase.h: two CFIndex (i64) fields, passed by value.
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CFRange {
        location: isize,
        length: isize,
    }

    const UTF8: u32 = 0x0800_0100;
    unsafe {
        let length = CFStringGetLength(value);
        if length <= 0 {
            return None;
        }
        // Measure the exact UTF-8 byte length first (null buffer, zero max):
        // sizing by UTF-16 units would under-allocate for multi-byte owners
        // (e.g. CJK app names) and let CFStringGetCString overflow.
        let mut used: isize = 0;
        let range = CFRange {
            location: 0,
            length,
        };
        let consumed =
            CFStringGetBytes(value, range, UTF8, 0, 0, std::ptr::null_mut(), 0, &mut used);
        if consumed != length || used <= 0 {
            return None;
        }
        let mut buffer = vec![0u8; used as usize + 1];
        let written =
            CFStringGetCString(value, buffer.as_mut_ptr(), (used + 1) as isize, UTF8);
        if written == 0 {
            return None;
        }
        Some(String::from_utf8_lossy(&buffer[..written as usize]).into_owned())
    }
}

#[cfg(target_os = "windows")]
fn verify_windows_foreground(browser: &BrowserKind) -> Result<(), InputError> {
    let expected = match browser {
        BrowserKind::Chrome => ["chrome.exe"],
        BrowserKind::Firefox => ["firefox.exe"],
    };
    for _ in 0..20 {
        if let Some(owner) = foreground_process_name() {
            if expected
                .iter()
                .any(|name| owner.eq_ignore_ascii_case(name))
            {
                std::thread::sleep(std::time::Duration::from_millis(100));
                return Ok(());
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(75));
    }
    Err(InputError::ForegroundNotTarget {
        actual: foreground_process_name().unwrap_or_default(),
    })
}

#[cfg(target_os = "windows")]
fn foreground_process_name() -> Option<String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };

    let window = unsafe { GetForegroundWindow() };
    if window.is_null() {
        return None;
    }
    let mut process_id: u32 = 0;
    unsafe {
        GetWindowThreadProcessId(window, &mut process_id);
    }
    if process_id == 0 {
        return None;
    }
    let handle = unsafe {
        OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id)
    };
    if handle.is_null() {
        return None;
    }
    let mut buffer = [0u16; 261];
    let mut size: u32 = buffer.len() as u32;
    let written = unsafe {
        QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut size)
    };
    let name = if written == 0 {
        None
    } else {
        Some(String::from_utf16_lossy(&buffer[..size as usize]))
    };
    unsafe {
        CloseHandle(handle);
    }
    name.and_then(|path| path.rsplit('\\').next().map(str::to_owned))
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
