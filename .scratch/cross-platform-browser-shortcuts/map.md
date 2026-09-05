# 跨平台雙分頁快捷鍵派送 App

Label: wayfinder:map

## Destination

產出可交付工程實作的技術與產品規格：可在 Windows、macOS Apple Silicon 與 Linux 打包執行，讓使用者選擇兩個 Chrome 或 Firefox 分頁，並對兩者發送同一快捷鍵。

## Notes

Domain: Browser Tab、快捷鍵、發送、目標選取；每次決策使用 `/grilling` 與 `/domain-modeling`。

Standing preferences:

- 目標是 Browser Tab，而非 OS 視窗。
- 快捷鍵是單一組合按鍵，例如 `F8` 或 `Ctrl+8`。
- 容許為發送依序切換前景並盡力恢復原焦點。
- 首版需有 Windows、macOS Apple Silicon、Linux 可執行封包；macOS Intel 不在支援矩陣。
- 目標無法唯一識別或已失效時，必須拒絕發送並要求重新選擇。

## Decisions so far

<!-- Closed child tickets are indexed here; each ticket owns its detailed resolution. -->
- [決定重複派送冷卻策略](issues/06-dispatch-cooldown-policy.md): 所有已接受的發送共用 60 秒全域冷卻，避免任何重複送出。
- [Browser Tab 跨瀏覽器整合研究](issues/01-browser-tab-integration-research.md): 採 browser-specific WebExtension + Native Messaging host；可管理 session-scoped tab 與 fail-closed 驗證，但無受支援的 trusted／原子雙目標快捷鍵發送。Brave 不屬首版支援範圍。
- [決定分頁目標與快捷鍵派送架構](issues/02-tab-targeting-and-dispatch-architecture.md): 桌面 App 協調、browser-specific extension 驗證／聚焦、allowlisted native host 依序 best-effort 注入；不使用 CDP，僅回報嘗試結果。
- [桌面技術堆疊與原生輸入研究](issues/08-desktop-stack-and-native-input-research.md): 選 Tauri 2 為可包裝 sidecar 的條件式首選，維持 browser-specific host installer；Windows 僅 chord stream 不交錯，macOS/Linux 均無 trusted/atomic 保證，Wayland 需 portal 授權。Brave 不屬首版支援範圍。
- [定義安全派送與失敗契約](issues/04-safe-dispatch-and-failure-contract.md): 目標必須不同；前景不可驗證即停止、不搶回焦點；只回報嘗試結果並以 `發送`作為唯一確認。
- [決定多個輸入標本的派送選取範圍](issues/11-input-specimen-selection-scope.md): 快捷鍵庫可有多個單一組合按鍵；每次發送只能選一個，兩個目標必須共用。
- [原型化雙分頁選取體驗](issues/05-two-tab-selection-experience.md): 採 Calibration Desk；左側快捷鍵庫，右側單一發送選一個共用快捷鍵，結果與冷卻可觀察。
- [決定桌面堆疊與輸入 adapter 選型](issues/09-desktop-stack-and-input-adapter-choice.md): Tauri 2 + Rust；獨立 host 經帶 capability token 的 OS-user IPC 連線，採平台 best-effort input 與 Tauri Store。
- [決定平台封包與支援矩陣](issues/03-platform-artifact-matrix.md): Windows NSIS、Apple Silicon 未簽署 DMG、Linux AppImage + `.deb`；商店 extension、手動桌面更新與 major-version fail-closed。
- [決定設定持久化與分頁重選規則](issues/07-persistent-preferences-and-session-targets.md): 僅持久化快捷鍵庫；目標與發送狀態皆為 session-only，冷卻隨 App 結束失效，設定 migration 可備份復原。
- [決定跨平台驗證與發布證據](issues/10-platform-validation-and-release-proof.md): 每個 release 需有 12 個平台／工作階段／browser 配對的實機證據（Chrome–Chrome、Firefox–Firefox、Chrome–Firefox）；Brave 不屬首版支援範圍。
- [修復 extension 一次連線失聯與快捷鍵逐字輸入](issues/12-extension-auto-reconnect-and-chord-recorder.md): extension 斷線後指數退避自動重試（啟動順序不再重要）、App 依 connected_hosts / last_bridge_rejection 分級診斷並清掉 stale session；快捷鍵組合改為按鍵錄製器（裸 Esc 取消、裸 F 鍵可單獨、其餘須帶修飾鍵），存儲格式不變。

## Not yet specified


## Out of scope

- 多步驟鍵盤巨集、文字輸入、滑鼠自動化與延遲流程：目的地僅涵蓋單一快捷鍵。
- macOS Intel 發行品：使用者明確排除。
- Brave 支援：首版排除（其 native host lookup 無官方路徑保證）；待官方確認或三平台實機證明完成後再行納入。
