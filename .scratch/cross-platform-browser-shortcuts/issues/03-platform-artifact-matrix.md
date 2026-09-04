# 決定平台封包與支援矩陣

Parent: ../map.md
Type: grilling
Status: resolved
Blocked by: none

## Question

在 Windows、macOS Apple Silicon 與 Linux 都提供可執行封包的既定範圍下，決定首版支援的 CPU 架構、封包格式、extension 與 native host 的安裝關係、更新機制責任，以及簽署與 notarization 是否屬於首版發行承諾。

## Answer

首版 artifact 為 Windows x64 的 NSIS installer、macOS Apple Silicon 的未簽署 DMG，以及 Linux x64 的 AppImage 與 `.deb`。各桌面 installer 安裝 native messaging host 與 browser-specific host manifest；不旁載 extension。

Chrome 從 Chrome Web Store 安裝 Chromium MV3 extension；Firefox 從 AMO 安裝具有固定 Gecko Add-on ID 的獨立且已簽署 extension。Brave 不屬首版支援範圍（其 native host lookup 無官方路徑保證），待官方確認或三平台實機證明完成後再行納入。

桌面 App 與 host 由使用者下載完整新版 installer 手動更新；extension 交由各商店更新。雙方連線須驗證 protocol major version，不相容即 fail-closed 拒絕連線。首版不提供 Windows Authenticode、Apple Developer ID／notarization 或 Linux 套件庫簽署；每個 release 發布 SHA-256 checksum，使用者自行驗證並處理 OS reputation／Gatekeeper 提示。

## Comments

- Context pointer: `.scratch/cross-platform-browser-shortcuts/map.md`
