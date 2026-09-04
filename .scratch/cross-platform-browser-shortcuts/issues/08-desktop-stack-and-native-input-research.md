# 研究桌面技術堆疊與原生輸入 adapter

Parent: ../map.md
Type: research
Status: resolved

## Question

以官方文件比較可發行至 Windows x64、macOS Apple Silicon、Linux x64 的桌面 App 技術堆疊。研究候選方案如何：封裝桌面 UI 與 native messaging host、提供本機設定儲存、支援 browser extension 的安裝流程，以及承載各平台 OS input adapter。確認 Windows、macOS、Linux 對輸入注入、可用桌面工作階段與權限的官方限制；不要把無文件保證的鍵盤 trusted／delivery 行為視為能力。

## Comments

- Context pointer: `.scratch/cross-platform-browser-shortcuts/map.md`

## Answer

- [桌面技術堆疊與原生輸入研究](../research/08-desktop-stack-and-native-input.md)：Tauri 2 是有條件的首選（可 bundling per-target sidecar／settings；browser host registration 仍由 installer 依 Chrome/Firefox 規則完成），Electron 是可行替代；Windows `SendInput` 只有同次 array 的 no-interspersing 保證，macOS CGEvent、X11 XTEST、Wayland Remote Desktop portal 都沒有 trusted 或 chord-atomic 文件保證。Wayland 要 user-granted portal session，X11 要 XTEST；Brave host lookup 缺第一方文件，維持 blocker。
