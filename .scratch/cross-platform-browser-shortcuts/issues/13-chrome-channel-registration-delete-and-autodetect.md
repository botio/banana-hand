# 13. Chrome-only 連線失敗、快捷鍵刪除與自動登錄

Type: task
Status: resolved

## Background

v0.1.2 修復了 extension 一次連線失聯（自動重試）與快捷鍵逐字輸入（改按鍵錄製），
但使用者回報三件事：

1. **Chrome extension 仍然連不到 App，Firefox 卻可以。**
   Firefox 能連＝App 端的 bridge（socket、token、dispatch）正常，問題在 Chrome
   側的 native host 發現。排查出兩個 Chrome 特有的成因：
   - **Chrome 各通道（stable／Beta／Canary）與 Chromium 的 native-messaging
     manifest 目錄各不相同**；v0.1.2 只寫 stable Chrome 的目錄。使用者若用
     Chromium（或 Beta／Canary），manifest 根本不在它的查找目錄，
     `connectNative` 永遠失敗。
   - **MV3 下 open-but-orphaned port 可能阻擋同 host 的後續 connect**；
     重試循環一旦持有已斷但未清掉的 port，Chrome 端可能卡死在「host not found」。
   - 另外，browser 對連不上 host 的原始診斷（`chrome.runtime.lastError`）
     之前沒有傳給 App，只能猜。

2. **快捷鍵庫每一項需要「×」刪除**——此前快捷鍵只能新增、不能刪除。

3. **「讓 Browser 找到 Host」整個面板應改為 App 啟動時自動連線偵測**，
   不需要使用者選 browser、按登錄；面板從 UI 移除。

## Decision

1. **自動登錄所有已知通道**：`native_host` 新增 `HostBrowser`
   （Chrome／ChromeBeta／ChromeCanary／Chromium／Firefox），
   `auto_register` 在 App `setup` 時把同一份固定-ID manifest 寫入所有通道的
   native-messaging 目錄（macOS／Linux 為檔案目錄；Windows 的 Chrome 系走
   registry、Firefox 改寫 `%LOCALAPPDATA%\Mozilla\Firefox\NativeMessagingHosts\`
   ——順帶修復 v0.1.x 把 Windows Firefox manifest 寫到錯誤位置的 bug）。
   逐通道失敗記在該 entry 上、不中斷批次；結果存 `AppState`，
   新的 `native_host_registration` Tauri command 供 UI 讀取。
   面板移除，改為 masthead 下的一行狀態（`#registration-status`）。
   manifest 中的 host 路徑是「App 執行檔同層」，因此每次啟動自動刷新，
   App 搬家後不需手動重登錄。
2. **斷線原因上報**：extension（Chromium＋Firefox）在 `onDisconnect` 捕獲
   `runtime.lastError.message` 存為 `lastDisconnectReason`，
   隨下一個 `hello` 以 `last_disconnect_reason` 送出（加性欄位，
   舊 App 忽略、`PROTOCOL_MAJOR` 不變）；App 存入
   `DispatchCoordinator.last_host_disconnect_reason` 並經 `runtime_snapshot`
   暴露；UI 在「尚無 native host 連線」分支附上
   「最近一次 extension 回報：{reason}」。握手成功（收到非 error 訊息）即清除。
3. **port orphan guard**：`connectNativeHost` 在建立新 port 前先
   `disconnect()` 舊 port，重試不再被卡住。
4. **快捷鍵刪除**：每張 card 右側 `×`（aria-label「刪除快捷鍵：{name}」）；
   `deleteShortcut` 走既有 `repository.replaceShortcuts`（存儲格式不變），
   刪除已選取項時選取落到剩餘第一項；save 進行中忽略點擊。
   UI 用 `event.stopPropagation()` 避免 label 的 radio 被連帶切換。
5. 快捷鍵庫、錄製器與所有既有行為不變；Playwright 新增刪除流程測試
   （第 5 支），既有 4 支全數保持。

## Validation

- `cargo test --workspace`：`native_host::tests` 新增
  `manifest_dirs_cover_every_chrome_channel`、
  `auto_register_writes_one_manifest_per_channel`；
  `integration_real_host_relays_browser_hello_to_app_and_back`
  的 hello 帶 `last_disconnect_reason` 並斷言 coordinator 收到。全綠。
- `npm run build`（tsc＋vite）全綠；兩支 extension `node --check` 通過。
- Playwright 5/5（含新刪除測試）。
- 本機 WebKit 原生 smoke：真 App 啟動後
  `/home/botio/.config/google-chrome/NativeMessagingHosts/` 與
  `~/.mozilla/native-messaging-hosts/` 出現 manifest（host 路徑指向
  `target/debug/` sibling）；UI 顯示「native host 已自動登錄：
  chrome、chrome-beta、chrome-canary、chromium、firefox」、
  舊面板消失、分級狀態與錄製器正常。
