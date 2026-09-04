# 商店 extension 與手動桌面更新

Chrome 使用 Chrome Web Store 的 Chromium MV3 extension；Firefox 使用具有固定 Gecko Add-on ID 的獨立 AMO 簽署 extension。桌面 installer 僅負責 native messaging host 與 browser-specific host manifest；桌面 App／host 採完整 installer 手動更新，extension 交由其商店更新，並以 protocol major version 不相容即拒絕連線。

## Consequences

首版交付 Windows x64 NSIS installer、macOS Apple Silicon 未簽署 DMG、Linux x64 AppImage 與 `.deb`。不提供 Windows Authenticode、Apple Developer ID／notarization 或 Linux 套件庫簽署；每個 release 提供 SHA-256 checksum，使用者自行處理 OS reputation／Gatekeeper 提示。Brave 不屬於首版支援範圍（其 native host lookup 無官方路徑保證）。