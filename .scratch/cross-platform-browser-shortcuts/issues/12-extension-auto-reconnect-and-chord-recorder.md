# 修復 macOS「extension 已載入但 App 連不到 Browser Tab」與快捷鍵逐字輸入問題

Parent: ../map.md
Type: task
Status: resolved

## Problem

v0.1.1 上線後使用者回報兩件事：

1. **macOS：App 連不到 Browser Tab（extension 已安裝）。** 根因是 extension 的
   `connectNativeHost()` 只執行一次（module load + `onStartup`）；native port 斷線後
   `onDisconnect` 只把 `nativePort` 清成 `undefined`，`scheduleSnapshot()` 見不到 port
   就靜默 return。任何「首次握手失敗」（App 尚未啟動、App 重啟後 capability token
   更換、Gatekeeper 擋下 sidecar）都會讓 extension 永久失聯，直到手動重新載入。
   同時 App 端只有「尚無已連線 Browser Tab」一句話，完全看不出卡在哪一級。
2. **快捷鍵組合欄位是純文字輸入**，使用者必須逐字打出 `Ctrl+Shift+K`；
   他期待的是「同時按下 ctrl+k，欄位就顯示出來」的按鍵錄製。

## Answer

**extension 自動重連（chromium + firefox background.js）：** `onDisconnect` 後
`setTimeout` 指數退避重試（3 秒起、上限 30 秒）；收到任何 host 訊息即重置退避；
收到 `type: "error"`（握手被拒，例如 stale token / protocol mismatch）時主動
`port.disconnect()` 觸發重試。native host 每次啟動都重讀 `bridge.json`，
所以 App 重啟換 token 後新握手自然成功——啟動順序不再重要。

**App 端分級診斷（bridge.rs + main.rs）：** `RuntimeSnapshot` 新增
`connected_hosts`（已握手的 browser session 數）與 `last_bridge_rejection`
（最近一次被拒握手的 code：`rejected_disconnected` / `protocol_mismatch` /
`invalid_message` / `unsupported_message`，成功 hello 時清除）。UI 依三段顯示：
無 host →「尚無 native host 連線」（附 rejection 說明）；有 host 無 tab →
「已連線但尚未收到快照，請重新載入 extension」；有 tab → 原文案。
host 斷線時同步清掉該 session 的 `browser_ports` 與 `connected_tabs`，
不再留下 stale tab。

**快捷鍵錄製器（src/main.ts）：** 組合欄位改為 capture 控制——點一下進入錄製，
window 層 `keydown`（capture phase，`preventDefault`）讀取 `event.code`
（layout-independent：`Key*`/`Digit*`/`F1-F24`）加上 `Enter`/`Tab`/`Space`/`Esc`，
修飾鍵順序固定 Ctrl+Shift+Alt+Meta；裸 `Esc` 取消；裸 F 鍵可單獨錄製，
其他主要按鍵必須帶至少一個修飾鍵（防誤抓）。錄製結果寫入隱藏
`<input name="chord">`，既有 `normalizeChord` / `parseChord` / FormData
流程與存儲格式完全不變（既有快捷鍵相容）。

**驗證：** `cargo test --workspace`（15 tests，含 real-host integration）全綠；
Playwright 4 tests 全綠（含新錄製器 regression：裸鍵拒絕、真實組合提交、
Esc 取消、pending-save 草稿不被 reset 洗掉）；`node --check` 兩支 background
通過。

## Comments

- Context pointer: 使用者 v0.1.1 上線後回報「mac app 還是連不到 browser tab，
  插件也裝了」與「快捷鍵組合應該是按鍵輸入不是文字輸入」。
- 發版：v0.1.2（2026-09-05）。
