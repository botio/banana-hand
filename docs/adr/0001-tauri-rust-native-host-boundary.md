# Tauri 2 與 Rust native host 邊界

首版以 Tauri 2 + Rust workspace 實作桌面 App、發送協調器與平台 input adapter；browser 啟動的 native messaging host 則是與 App 共用 core／協定 crate 的獨立 binary。host 僅以 OS-user scoped named pipe 或 Unix domain socket 加上每次 App 啟動的 capability token 連接 App，不開放 loopback TCP；這保留 Native Messaging 的程序邊界與最小權限，而不把 browser stdin/stdout 協定混入長駐 UI 程序。

## Consequences

Windows 使用 SendInput；macOS Apple Silicon 使用已授權的 CGEvent；Linux 的 X11 路徑使用 XTEST，Wayland 路徑目前以 fail-closed 權限門拒絕發送（Remote Desktop portal 已立規格、尚未實現），任何 Permission Gate 不可用時一律拒絕。