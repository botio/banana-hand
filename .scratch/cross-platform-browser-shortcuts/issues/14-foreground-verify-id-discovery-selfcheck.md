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

更正：本輪把 Chrome 問題歸因於「未完整重啟」的推論不成立。
manifest 內容與 extension ID 相符，不代表登錄目錄正確；真正的
macOS 使用者路徑必須包含 `Library/Application Support/Google/Chrome`。

## Comments

### 2026-09-06：v0.1.6 回報的根因與修正

- Chrome：混淆 `/Library/Google/Chrome` 系統層路徑與使用者層路徑，
  導致自動登錄漏掉 `Application Support`。stable／Beta／Canary
  全部修正，並加入可在 Linux 執行的 macOS 目錄回歸測試。
  依據：[Chrome 官方 native messaging 文件](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging)。
- 重啟不重連：以真實 native-host 程序重現，App socket 關閉但 browser
  stdin 保持開啟時，host 不退出。原雙 channel 輪詢忽略 desktop channel
  的 Disconnected；改為同一事件 channel，EOF／讀取錯誤終止 host，
  讓 browser 收到 onDisconnect 並沿用既有重試流程。
- 同一 relay 邊界原本每行重建 BufReader，會丟掉預讀的下一筆訊息。
  改為整個連線保留同一 reader；以多筆 response 合併寫入驗證順序及完整性。
- 發送診斷：Tauri `Result<_, String>` 的拒絕值是字串，前端卻只保留
  Error.message，其他值全改成「發送請求失敗」。回歸測試先失敗、
  修正後能顯示原始拒絕原因；未把實際 Mac 的拒絕原因當成已知。
- macOS 前景閘門：手寫 EXCLUDE_DESKTOP=2 不正確；2 是
  OnScreenAboveWindow，ExcludeDesktopElements 是 1<<4。
  改用既有 core-graphics crate 的常數，不再手寫這組 flag。
- self-check 僅是啟動時讀 config 並嘗試建立 socket／pipe 連線，
  不做 token 握手，也不驗證 Chrome discovery、持續存活或輸入權限。
  `ok` 與上述故障可以同時存在。

驗證：native-host 程序回歸涵蓋 EOF、讀取錯誤與合併訊息；
真實 Firefox background.js 搭配真實 native-host 與模擬 browser API／
App socket，重啟後無需 Tab 活動或重載即可用新 token 重新 hello。
這不是 macOS Firefox 實機輸入驗證；Mac 前景與快捷鍵送達仍待實機確認。

同輪補齊 extension 生命週期：local `port.disconnect()` 不會觸發自身
onDisconnect，因此握手拒絕分支必須主動清掉 port 並排程重連；
兩個 browser 均有先失敗、修正後通過的回歸測試。Firefox manifest
是非 persistent event page，新增與 Chromium 相同的每分鐘 alarm，
避免 App 長時間未開時 setTimeout 隨背景頁卸載而消失。Firefox 斷線
原因改讀正式 API 的 `port.error`，不再讀 Chrome 專用 lastError。

### 2026-09-06：v0.1.7 實機回報——前景不是 browser（「目前前景：G」）

v0.1.7 上 Chrome／Firefox 都連上了，但發送皆被前景閘門拒絕，
錯誤文字顯示「目前前景：G」（使用者的終端機／聊天程序，單字
owner name）。機制：使用者在 browser 之外的 App 中操作 Banana
Hand，而 WebExtension 的 `windows.update({ focused: true })` 只
能在該 browser 已是前景 App 時生效——browser 停留在背景，閘門
按設計 fail-closed（閘門本身沒壞，錯誤文字也第一次顯示了真正
原因）。

修正（ADR 0004，v0.1.8）：macOS `InputAdapter::activate` 在
in-browser 啟用／聚焦與閘門之前，用 `ps -eo command` 挑出正在
執行的 Chrome 系 App（stable 優先、絕不啟動未執行的 App）再
`open -a`；找不到即整次拒絕。候選匹配是純函式
`pick_running_candidate`，Linux/Windows/macOS 都有測試。

限制：同時執行多個 Chrome channel 時只能挑一個（stable 優先），
目標 tab 若在另一 channel 可能送到同系錯誤 window——preview
階段限制。Windows `activate` 為 no-op（未回報問題前不加 FFI）。

仍待：使用者 Mac 實機確認（快捷鍵實際送達、`G` 的實際程序
名稱）。
