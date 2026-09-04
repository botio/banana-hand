# 決定分頁目標與快捷鍵派送架構

Parent: ../map.md
Type: grilling
Status: resolved
Blocked by: 01

## Question

根據「研究 Browser Tab 跨瀏覽器整合途徑」的已驗證能力，選擇首版的 Browser Tab 枚舉、識別、啟用與 Shortcut Chord 派送架構。決定桌面 App、瀏覽器 extension、native messaging host 與 OS 輸入層各自的責任、最小權限，以及無法提供的保證。

## Answer

首版採用桌面 App 作為協調器的 best-effort 架構：

- **桌面 App**：擁有 Target Selection、App-wide Cooldown、按目標的結果呈現與依序派送流程；所有 user-visible 狀態以它為準。
- **Browser-specific WebExtension**：每個 Connected Browser 各自安裝；只列舉所屬 profile 的 Browser Tab、維護 session-scoped identity、在每次派送前重新驗證並啟用／聚焦指定目標。
- **Native Messaging host**：只接受 extension allowlist 的 IPC，提供桌面 App 與 extension 的橋接，並承載平台原生輸入 adapter。
- **OS input adapter**：對 ready target 依序嘗試注入 Shortcut Chord；第一個注入前任何驗證／聚焦失敗即中止。第一個已嘗試後才失敗，停止後續目標並回報 partial outcome；絕不自動重試。

每個瀏覽器可獨立安裝 extension，App 只列出已連線的 browser；至少有兩個可選 Browser Tab 才能執行。extension 使用 `tabs` 與 `nativeMessaging` 最小權限，即時顯示 browser、window 與 tab title；title、URL、內容均不持久化、不離開裝置。首版不使用 CDP／Debugger。

此架構只保證可驗證、啟用並**嘗試**依序派送，不保證 trusted event、送達、原子性、同時性或 exactly-once。缺少 Permission Gate 要求的系統權限、或工作階段不支援輸入注入時，執行必須被阻止並顯示修復資訊。Brave 只有完成三平台 extension、native host、連線與派送實測後才可宣稱受支援。

## Comments

- Context pointer: `.scratch/cross-platform-browser-shortcuts/map.md`
