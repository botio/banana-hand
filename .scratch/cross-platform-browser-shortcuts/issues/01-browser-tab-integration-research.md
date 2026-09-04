# 研究 Browser Tab 跨瀏覽器整合途徑

Parent: ../map.md
Type: research
Status: resolved

## Question

在 Windows x64、macOS Apple Silicon、Linux x64 上，哪一種可安裝的瀏覽器整合與本機桌面橋接組合，能讓 App 對 Chrome、Firefox：

1. 枚舉並唯一識別使用者可選取的 Browser Tab；
2. 將指定 Tab 置為可接收輸入的目標；
3. 對兩個選取目標派送同一 Shortcut Chord；
4. 在目標失效時安全地拒絕派送。

請以各瀏覽器官方 WebExtensions／Native Messaging／automation 文件，確認 API 能力、安裝與權限需求、平台差異，以及無法保證的行為。研究必須指出是否能派送可被網頁或瀏覽器可信任的原生快捷鍵。

## Answer

採用「每個瀏覽器各一個 WebExtension + 由桌面 installer 部署、Native Messaging allowlist 限制的本機 host」：extension 可在其 browser session 內列舉/驗證 tab、啟用 tab 並要求其 window 聚焦；tab identity 必須含 browser/profile-session nonce，不能跨重啟永久保存。派送前可 fail-closed 拒絕已移除、替換、斷線或無法重新驗證的目標，但不存在跨 browser 的原子 delivery 保證。WebExtensions/Native Messaging 沒有向任意 tab 派送快捷鍵的 API；synthetic DOM keyboard event 是 untrusted，OS 級依序注入也未獲瀏覽器文件保證為 trusted。Brave extension 可用 Chromium MV3，但其 Native Messaging host lookup 沒有找到 Brave 官方文件，故 Brave 不屬首版支援範圍。完整來源、平台矩陣與不確定性見 [研究資產](../research/01-browser-tab-integration.md)。

## Comments

- Context pointer: `.scratch/cross-platform-browser-shortcuts/map.md`
