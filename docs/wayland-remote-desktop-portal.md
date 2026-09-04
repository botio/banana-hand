# Wayland Remote Desktop portal — 輸入注入規格

## 狀態

**Release blocker / spec-only。** 目前 `src-tauri/src/input.rs` 對 `XDG_SESSION_TYPE=wayland`
維持 fail-closed：回傳 `InputError::PortalPermissionRequired`，不嘗試任何注入。
本文件是接上 portal 前的精確規格，不是已實作且已驗證的路徑。在取得 ADR 0003
24-cell 發布證據矩陣中 Wayland 列的實機證據之前，不可宣稱 Wayland 支援。

## 為什麼 Wayland 不一樣

X11 可以用 XTEST 對全螢幕直接偽造 key event，因為 X server 是共享的。Wayland
沒有等價的全局注入點：輸入屬於 compositor，一個 app 要送 key event 必須經過
compositor 認可的通道，而 `org.freedesktop.portal.RemoteDesktop`（XDG Desktop
Portal 的一部分）就是官方認可的通道。它同時是**安全機制**——compositor 會彈出
對話窗請使用者授权，無法靜默注入。

另有一個更上游的约束：Wayland 下普通 app **無法註冊全域 hotkey**（沒有等價
X11 `XGrabKey` 的協定）。本 App 的「按下快捷鍵就觸發 dispatch」前提，在 Wayland
上必須改成「在 App 內觸發」或改用 `org.freedesktop.portal.GlobalShortcuts`。
這是輸入注入之前的独立問題，但同樣屬於 Wayland 發布 blocker。

## 端點

```
Destination : org.freedesktop.portal.Desktop
Object path : /org/freedesktop/portal/desktop
Interface   : org.freedesktop.portal.RemoteDesktop
```

portal 方法都是**非同步**：回傳一個 `org.freedesktop.portal.Request` 的 object
path，真正的結果透過該 Request 的 `Response` signal 回來（成功 `response = 0`，
失敗帶 error name）。例外是 `Notify*` 事件方法與 `ConnectToEIS`，它們是直接
呼叫。

## 最小鍵盤注入流程（D-Bus Notify 路徑）

依序呼叫（`a{sv}` 是 vardict；`s`/`o`/`u`/`i`/`h` 是 D-Bus 型別）：

1. `CreateSession(options) -> handle`
   - `options` 可含 `handle_token`、`session_handle_token`。
   - `Response` 回 `session_handle`（`s` 型，但其實是 object path）。

2. `SelectDevices(session_handle, options) -> handle`
   - `options.types`（`u`）bitmask：**`1` = KEYBOARD**、`2` = POINTER、`4` = TOUCHSCREEN。
   - 純鍵盤取 `types = 1`。
   - 可用 `AvailableDeviceTypes` property（`u`，read）先查 compositor 支援哪些。

3. `Start(session_handle, parent_window, options) -> handle`
   - `parent_window`：portal window id，沒有就傳空字串 `""`。
   - **這是使用者授权點**：compositor 彈 dialog。
   - `Response` 回 `devices`（`u` bitmask，實際授权的裝置）與 `clipboard_enabled`（`b`）。
   - 使用者拒絕 → Request 回 error（例：`org.freedesktop.portal.Error.Denied`）；
     授權成功且 `devices` 含 bit `1` 才能送鍵盤。

4. `NotifyKeyboardKeycode(session_handle, options, keycode, state)`
   - `keycode`（`i`）：**Linux evdev/input keycode**（`A` = 30 / `KEY_A`），**不是**
     X11 keycode，也**不是** XKB keycode。
   - `state`（`u`）：`0` = released、`1` = pressed。
   - 一個 chord = 對每個 modifier 先 press、再 press 主 key、依序 release；
     跟 X11 XTEST 的 press/release 串流同理，只是通道與 keycode 空間不同。
   - 或改用 `NotifyKeyboardKeysym(session_handle, options, keysym, state)`：
     `keysym`（`i`）是 XKB keysym——更接近本專案 `ShortcutChord` 已有的 keysym
     空間，但 `NotifyKeyboardKeysym` 的 compositor 支援度較低，需以
     `AvailableDeviceTypes` / 版本確認。

5. 結束：對 session object path 呼叫 `org.freedesktop.portal.Session.Close()`。

`gdbus` 手動示例（鍵盤 `A`，keycode 30）：

```sh
gdbus call --session \
  --dest org.freedesktop.portal.Desktop \
  --object-path /org/freedesktop/portal/desktop \
  --method org.freedesktop.portal.RemoteDesktop.CreateSession \
  "{ 'session_handle_token': <'bananahand-1'> }"
# → 取得 session object path，再 SelectDevices(types=1)、Start("")，
#   等 Response；成功後：
gdbus call --session \
  --dest org.freedesktop.portal.Desktop \
  --object-path /org/freedesktop/portal/desktop \
  --method org.freedesktop.portal.RemoteDesktop.NotifyKeyboardKeycode \
  <session-path> "{}" 30 1   # press A
gdbus call --session \
  --dest org.freedesktop.portal.Desktop \
  --object-path /org/freedesktop/portal/desktop \
  --method org.freedesktop.portal.RemoteDesktop.NotifyKeyboardKeycode \
  <session-path> "{}" 30 0   # release A
```

## 較新的 EIS 路徑（version 2）

`ConnectToEIS(session_handle, options) -> fd`：回傳一個 Unix fd，交给
`libei` sender context。一旦建立 EIS 連線，**只能走 EIS**，`Notify*` 方法會
回 error。現代 GNOME（48+）偏好 EIS。對「最小實作」而言，D-Bus `Notify*`
路徑較容易起步；但若要相容 GNOME 新版的強制 EIS，就得引入 `libei` FFI，
複雜度與不可驗證面大增。

## 最小 Rust 實作需要什麼

- 一個 D-Bus client（`zbus` / `dbus-next` / `dbus`）+ 能处理非同步 `Response`
  signal 的 event loop。
- `ShortcutChord`（keysym 空間）→ evdev `keycode`（或 XKB `keysym`）的映射表；
  與 X11 的 `keycode(display, keysym)`、Windows 的 VK 各不相同，要独立维护。
- 授权状态機：`Start` 前是「未授权」，成功授权後 session 可送事件，被拒或
  session 斷掉就回到 fail-closed。
- session 的生命期管理（`Close`）與 fd/session handle 的清理。

## 為什麼它仍是 release blocker（而非本次直接實作）

1. **不可在此驗證**：本機沒有帶可用 RemoteDesktop portal backend 的 Wayland
   compositor；`Start` 的授权 dialog、`AvailableDeviceTypes`、EIS vs Notify
   的行為都必須在真實 compositor 上跑過才算數。盲寫 zbus/libei 會產出一堆
   無法確認的 FFI。
2. **compositor 依賴**：portal backend 由 compositor 實現（GNOME 走
   xdg-desktop-portal-gtk、KDE 走 xdg-desktop-portal-kde、Sway/Hyprland 各自
   不同），有的根本沒實作 RemoteDesktop——那种環境下 `CreateSession` 直接失敗，
   只能 fail-closed。
3. **授权是使用者互動**：`Start` 要等使用者点「允許」，跟本 App「按快捷鍵就
   尽力發送」的 fire-and-forget 模型衝突；Wayland 下無法静默注入，這是安全設計。
4. **上游的 hotkey 問題**：Wayland 無全域 hotkey，觸發方式要先解决（見上）。

## 決策

維持 `InputError::PortalPermissionRequired` fail-closed 閘門作為**安全預設**。
要解開它：先做「App 內觸發 + GlobalShortcuts portal」的觸發設計，再實作
D-Bus `Notify*` 最小注入（keysym→evdev keycode 映射），在 ≥1 個主流 compositor
（建議 GNOME 與 Sway 各一）跑通 `CreateSession→SelectDevices→Start→
NotifyKeyboardKeycode→Close` 並留下證據；EIS 路徑另行評估是否必要。
