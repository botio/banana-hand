# 14. macOS 發送落到錯誤視窗、Chrome 永遠連不上：前景驗證 + ID 偵測 + self-check

Type: task
Status: resolved

## Background

v0.1.3 之後使用者回報（macOS、真 Chrome）：

1. **App 顯示「已嘗試發送」，但按鍵落在另一個 app**——目標分頁的視窗有切過去，
   快捷鍵卻没進該分頁。根因：macOS 視窗激活是**非同步**的——
   extension 回報 `prepared {ready:true}` 只代表「要求」已被接受，
   window server 尚未完成切換時，App 立刻 post 的 HID 事件仍送往
   **先前**的 foreground app。v0.1.x 的 dispatch 流程完全沒做前景驗證
   （ticket 04 契約要求「前景不可驗證即停止」）。
   同一使用者在「輔助功能」清單有一條**沒打勾**的 Banana Hand——
   ad-hoc 簽章每次重新編譯都是新的 CDHash，舊條目不匹配；而我們的
   `AXIsProcessTrusted()`（非提示版）只会靜默失敗，把使用者丟給一段
   自己得去翻的設定文字。
2. **Chrome（stable，非 Chromium）仍連不上**，而 v0.1.3 已把 manifest
   寫進 stable Chrome 目錄。排查出兩個 Chrome 特有的隱藏層：
   - **extension ID 錯配**：manifest 的 `key` 是 **v0.1.1 才加入**的；
     在此之前解壓安裝的 extension（v0.1.0）拿到的是**路徑派生的 ID**，
     與固定 ID `mooakjhlbkjfbmbmliklkmfmacnomlai` 不同。Chrome 對
     `allowed_origins` 不包含呼叫 extension 的 manifest 直接跳過
     （lastError 仍顯示 "Native messaging host not found"）——
     對這種 installation，寫再多目錄都没用。
   - **MV3 service worker 死亡**：Chrome 約 30 秒 idle 就收回 service
     worker；掛著的 `setTimeout` 重試不會保活。worker 一旦死在重試中途
     就再也不會重試（Firefox MV2 background page 是永駐的，所以只有
     Chrome 出這個問題）。

## Decision

1. **前景驗證（macOS + Windows）**：`InputAdapter` 新增
   `verify_foreground(browser)`（Linux 預設 no-op）。macOS 用
   `CGWindowListCopyWindowInfo` 讀第一支 layer-0 窗口的 `kCGWindowOwnerName`，
   Windows 用 `GetForegroundWindow` → `QueryFullProcessImageNameW`
   （windows-sys 0.61：`OpenProcess`/`QueryFullProcessImageNameW` 在
   `Win32_System_Threading`，`GetWindowThreadProcessId`/`GetForegroundWindow`
   在 `Win32_UI_WindowsAndMessaging`）。送出前輪詢最多 ~1.5 秒；
   不匹配（含超时）→ 新的 `ForegroundNotTarget` fail-closed 拒絕。
2. **macOS Accessibility 提示**：改用 `AXIsProcessTrustedWithOptions`
   （`kAXTrustedCheckOptionPrompt=true`）——未授權時彈系統原生對話框
   （含跳轉系統設定），不再只有文字錯誤。
3. **extension ID 自動偵測**：`native_host::discover_extension_ids`
   掃描各 Chrome channel 的 profile `Extensions/<id>/<version>/manifest.json`
   （macOS `~/Library/Application Support/Google/Chrome*/`、
   Linux `~/.config/google-chrome*/`、Windows `%LOCALAPPDATA%\.../User Data`），
   以 name == "Banana Hand Browser Bridge" 判定，發現的 ID 全部寫進
   `allowed_origins`（固定 ID 永遠排第一、去重）。
4. **MV3 keepalive**：chromium extension 加 `chrome.alarms`
   （`connect-watch`，periodInMinutes: 1）——alarm 是保證能喚醒
   service worker 的事件，worker 死後最多一分鐘重試循環重生；
   `onAlarm` 同時把 backoff 重置為 base。`onDisconnect` 的 lastError
   另 `console.warn`（port 從未建立時 App 收不到回報，console 是唯一
   可查處）。
5. **native host self-check**：host binary 新增 `--self-check`
   （讀 `bridge.json` + 連 socket，成功 exit 0、失敗非 0 附原因）；
   App 啟動時跑一次並存進 `AppState.host_self_check`，經
   `runtime_snapshot.host_self_check` 進入狀態列（「native host
   self-check：ok/failed…」）。macOS 上 Gatekeeper 殺 host（SIGKILL、
   無輸出）會被辨識成「operating system 殺掉，通常是隔離屬性」並給
   `xattr -dr` 指令。
6. Firefox extension 不改（MV2 background 永駐，无 worker 問題）。

## Validation

- `cargo test --workspace`：`native_host::tests` 新增
  `discovery_adds_installed_extension_ids_to_allowed_origins`
  （fake profile 樹、noise extension 過濾、去重）；既有 15 支全綠。
- `npm run build`（tsc + vite）全綠；兩支 extension `node --check` 通過；
  Playwright 5/5（新 `host_self_check` 欄位為加性，stub 不受影響）。
- 本機 WebKit 原生 smoke：真 App 啟動後 `host_self_check` 顯示 ok、
  manifest 含偵測欄位。macOS/Windows FFI 本開發機無工具鏈，
  由 CI（macos-latest / windows-latest）建置驗證。
- Playwright 5/5（新 `host_self_check` 欄位為加性，stub 不受影響）。

## v0.1.4 regression → v0.1.5 fix

v0.1.4 上線後使用者回報「一按發送 App 就崩潰」（macOS）。根因：新加的
CoreFoundation `extern "C"` 簽名是按記憶寫的、未經目標平台編譯：
`CFStringCreateWithCString` 真实是 **3 參** `(allocator, string, encoding)`，
被声明成 2 參 `(encoding, string)`——呼叫時 allocator 槽位讀到 encoding
整數（假指標），CF 把它當 CFAllocator 物件解參數 → **第一次 call 就
segfault**（a11y gate 與 window-owner 查詢兩條路徑都踩）。
另兩個隱性錯誤一併修掉：`CFStringGetCString` 的 buffer 以前按
**UTF-16 長度**分配（CJK 名稱會溢出）、`CFNumberGetValue`/`CFString`
沒有 type guard（非预期類型未定義行為）。

v0.1.5 修法：所有 CF 簽名對齊官方 header（`CFStringCreateWithCString`
帶 null allocator、`CFStringGetCStringLength` 取**精確 UTF-8 位元組長**
再分配 buffer、`CFGetTypeID` vs `CFNumberGetTypeID`/`CFStringGetTypeID`
先驗類型再取值）。驗證方式：把 FFI 區塊原樣抽出成獨立 probe crate，
對 `aarch64-apple-darwin` / `x86_64-pc-windows-gnu` 跑 `cargo check`
（本機無 macOS/Windows 工具鏈時的可行替代；link-level 錯誤如
`CFBooleanCreate` 不存在仍只能靠 CI——那個已在 v0.1.4 輪次被抓出並修掉）。

同輪又抓到兩個只能靠對照來源發現的錯誤（CI linker 抓到
`CFStringGetCStringLength`——該函數**根本不存在**，CF 沒有這種 API；
改為用真正的 `CFStringGetBytes(string, CFRange, encoding, …)`，
null buffer + max 0 先量出精確 UTF-8 位元組長再分配 buffer）。
另：`kCFNumberSInt64Type` 是 **4**（不是 6，6 是 Float64）——若照 6
呼叫，`CFNumberGetValue` 把 float64 位元組模式當 i64 讀回，layer-0
窗口永遠「不匹配」，前景驗證在 macOS 上必然 fail-closed。所有 CF 符號
已逐一對照 `core-foundation-sys`（本機 cargo registry 內的官方綁定
轉寫）確認存在且簽名一致。

## v0.1.6：extension 競態 + self-check 可見性

使用者回報（v0.1.5 版）：(1) Chrome console 大量
`TypeError: Cannot read properties of undefined (reading 'postMessage') at
sendSnapshot`；(2) 重啟 App 後 Firefox 不會自動連上。

**Race**：`sendSnapshot`／`prepareTarget` 的 guard 只在函式頂部（await
之前）檢查 `nativePort`；`await chrome.windows.getAll()`／`tabs.get`
期間 host 死掉 → onDisconnect 把 `nativePort` 設 undefined → await 恢復
後 `nativePort.postMessage` 對 undefined 呼叫 → 未捕獲 rejection（即
使用者看到的 TypeError 暴增；prepareTarget 同一 race 會讓 `prepared`
訊息送不出去、App 端等不到而超時——可能是「發送請求失敗」的來源之一）。
修法：新增 `post(message)` helper（先查 `nativePort`、try/catch 包住
postMessage——部分 browser build 對已關閉 port 的 postMessage 會丟
exception），所有 postMessage 呼叫點（snapshot、prepared ×3、hello）改走
helper；onDisconnect 的重試排程不變。另給 Firefox onDisconnect 補上與
Chrome 相同的 `console.warn`（之前 Firefox 把 `lastDisconnectReason`
只存進變數、使用者完全無從看見 port 為什麼死）。

**Self-check 可見性**：App 狀態列只在「尚無 native host 連線」分支顯示
`native host self-check：…`——host 已連線（例如 stale port 還掛著）時
完全看不到，剛好遮蔽「App 端 bridge 壞了但 extension 還連著」這類故障
（本次 Firefox 重啟不連線的診斷就被它卡住）。修法：`refreshRuntime`
把 self-check 字串提到 if-chain 之前、三個分支（有 Tab、0 host、有 host
無 Tab）一律附上。

Chrome「Specified native messaging host not found」且 manifest／ID 全部
正確時，指向 Chrome browser process 從未完整重啟（host 清單只在
browser process 啟動時讀取）——v0.1.6 不改此路徑，靠上面的自我診斷
行 + 使用者完整重啟 Chrome 驗證。
