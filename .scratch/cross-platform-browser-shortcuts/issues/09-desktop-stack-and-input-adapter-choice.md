# 決定桌面堆疊與輸入 adapter 選型

Parent: ../map.md
Type: grilling
Status: resolved
Blocked by: 08

## Question

根據「研究桌面技術堆疊與原生輸入 adapter」的文件化能力，選定桌面 UI／協調器、native messaging host、設定儲存與 Windows、macOS、Linux input adapter 的首版實作堆疊。選型必須支援已決定的 best-effort 行為、Permission Gate、三個目標平台與 browser-specific extension 安裝，不得以未證實的 trusted input 承諾取代限制。

## Answer

首版採 Tauri 2 + Rust workspace。Tauri App 是發送協調器；各瀏覽器的 native messaging host 是與 App 共用 core／協定 crate 的獨立 Rust binary，並透過 OS-user scoped named pipe（Windows）或 Unix domain socket（macOS、Linux）連線。每次 App 啟動產生 capability token；禁止 loopback TCP 與無 token 連線。

原生輸入 adapter 依平台拆分：Windows 用 SendInput；macOS Apple Silicon 用 CGEvent 與 Accessibility 權限；Linux 同時支援 X11 XTEST 及受使用者授權的 Wayland Remote Desktop portal。所有 adapter 都維持 best-effort 與 Permission Gate；不可用即拒絕發送。macOS 發行為未簽署、未 notarize 的直接下載 DMG，不使用 Developer ID；使用者自行經 Gatekeeper「仍要打開／Open Anyway」與 Accessibility 授權。

設定機制採 Tauri Store plugin，放在 OS 慣例設定目錄；其中可持久化的欄位、快捷鍵庫與 schema 演進仍由後續設定票決定。

## Comments

- Context pointer: `.scratch/cross-platform-browser-shortcuts/map.md`
