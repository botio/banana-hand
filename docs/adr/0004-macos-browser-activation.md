# macOS 桌面 App 在發送前把目標瀏覽器 App 帶到前景

桌面 App 擁有使用者完整權限，可以代為拉起前景，故由 `InputAdapter::activate`（macOS 實装）在 in-browser 啟用／聚焦與前景閘門之前執行：先以 `ps -eo command` 在 Chrome 系四個候選名（`Google Chrome`、`Google Chrome Beta`、`Google Chrome Canary`、`Chromium`）中挑出**正在執行**的那一個、Firefox 則為 `Firefox`，再 `open -a`；找不到正在執行的 browser 或 `open` 失敗時整次發送被拒，绝不啟動未執行的 browser。候選匹配是純函式（`pick_running_candidate`），在非 macOS 平台也有測試覆蓋。

## Consequences

前景閘門本身維持 verify-only 不變：它只在輸入嘗試前驗證、失敗立即停止且不搶焦點；App 層的前置激活屬於「盡力原生發送」的啟用步驟，不是對閘門失敗的救贖。Windows／Linux 的 `activate` 是 no-op（Windows 上 MV3 `windows.update` 可直接拉起前景，未回報問題前不加 FFI）。同時執行多個 Chrome channel 時，`open -a` 只能挑一個，若目標 tab 在另一 channel 则可能把快捷鍵送到同系但錯誤的 window；preview 階段記錄為限制，候選順序以 stable 優先。
