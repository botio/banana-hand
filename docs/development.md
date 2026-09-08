# 開發與打包說明

本文件給需要修改、編譯或維護 Banana Hand 的開發者。一般使用者請看 [README](../README.md) 與[插件安裝教學](browser-extension-installation.md)，不需要執行這裡的指令。

## 目錄

- [環境需求](#requirements)
- [本機開發](#dev-loop)
- [程式結構與發送契約](#layout)
- [native host 與 manifest](#native-host)
- [打包與發布](#packaging)
- [測試與驗證](#testing)
- [發布證據與限制](#verification)

<a id="requirements"></a>

## 環境需求

- Node.js 與 npm；目前 CI 使用 **Node.js 22**，重現 CI 建議使用相同版本。
- Rust stable 與目前平台需要的原生編譯工具鏈。
- Python 3：插件打包 script 使用 `python3` 與標準庫 `zipfile`。
- Linux：Tauri 2 所需 GTK 3、WebKitGTK 4.1 等開發套件，以及 X11／XTEST 開發函式庫。CI 的套件列表可查 [build.yml](../.github/workflows/build.yml)，本機依發行版補齊。
- Windows：MSVC C++ 建置工具、Windows SDK，以及 `x86_64-pc-windows-msvc` Rust target。
- macOS：Apple 開發工具鏈；目前發布 target 為 `aarch64-apple-darwin`。

僅安裝 Rust 的交叉編譯 target，不代表已備妥跨平台連結器或 SDK。桌面安裝檔目前由各平台的原生 runner 建置，不應將本機交叉 `cargo check` 當成打包成功。

<a id="dev-loop"></a>

## 本機開發

在專案根目錄執行：

```sh
npm ci
npm run bundle-native-host
npm run tauri -- dev
```

這三步各自負責：

1. `npm ci` 依 lockfile 安裝前端與開發工具。
2. `bundle-native-host` 編譯 release native host，放到 `src-tauri/binaries/`，採 Tauri 要求的 target-suffixed 檔名。
3. `npm run tauri -- dev` 使用專案安裝的 Tauri CLI，啟動桌面 App；`beforeDevCommand` 會啟動 Vite。

**不要省略 sidecar 準備。** `beforeDevCommand` 目前只執行 `npm run dev`；單獨 `cargo build -p banana-hand-native-host` 只會生成一般 debug binary，不會完成 `src-tauri/binaries/` 的 sidecar staging。native host 原始碼有變動時，重新執行 `npm run bundle-native-host`。

只看前端、不連接桌面協調器：

```sh
npm run dev
```

這種瀏覽器預覽沒有 Tauri 桌面連線，不能用來證明分頁列舉、native host 或原生輸入正常。

<a id="layout"></a>

## 程式結構與發送契約

| 路徑 | 責任 |
| --- | --- |
| [src/](../src/) | TypeScript 前端：快捷鍵庫、錄製、目標選取與狀態呈現 |
| [src-tauri/src/main.rs](../src-tauri/src/main.rs) | 發送協調器、Tauri commands、目標準備、全域冷卻及結果 |
| [src-tauri/src/bridge.rs](../src-tauri/src/bridge.rs) | Unix socket／Windows named pipe、連線驗證、分頁與準備回覆 |
| [src-tauri/src/input.rs](../src-tauri/src/input.rs) | 各平台的啟用、前景驗證及原生輸入 |
| [src-tauri/src/native_host.rs](../src-tauri/src/native_host.rs) | host manifest 自動登錄與啟動自我檢查 |
| [crates/protocol/](../crates/protocol/) | 共用通訊型別、請求驗證與 runtime 目錄選擇 |
| [crates/native-host/](../crates/native-host/) | 瀏覽器啟動的 sidecar，轉送 native messaging 與桌面通訊 |
| [extensions/chromium/](../extensions/chromium/) | Chrome MV3 service worker |
| [extensions/firefox/](../extensions/firefox/) | Firefox MV3 非持續性背景事件頁，固定 Gecko ID |
| [scripts/](../scripts/) | sidecar／插件打包、manifest 產生與 Windows smoke |
| [tests/](../tests/) | 前端及插件回歸測試 |

發送由桌面 App 協調；插件負責列舉及準備目標分頁，native host 負責訊息轉送，實際輸入由桌面 `input.rs` 執行。

- 兩個目標需不同且仍連線，同一次請求使用同一個快捷鍵。
- 發送依序處理目標，不保證原子雙目標、送達或 exactly-once。
- 目標準備、前景驗證與注入是不同階段；錯誤不能混稱為同一種「逾時」。
- 第一個輸入嘗試成功返回後開始 60 秒全域冷卻；第二個準備失敗需保留第一個已嘗試的 partial outcome，不自動重試。
- 快捷鍵庫持久化；目標、連線識別資訊、發送結果與冷卻為當次執行狀態。

完整術語見 [CONTEXT.md](../CONTEXT.md)；面向使用者的結果解讀見 [README](../README.md#results)。

<a id="native-host"></a>

## native host 與 manifest

App **啟動時**自動登錄 host manifest，並對內附 host 執行一次 `--self-check`。installer 負責分發桌面程式與 sidecar，不能因此宣稱使用者的瀏覽器插件也已安裝。

host 名稱為 `dev.bananahand.dispatch_host`。標準安裝路徑由 App 自動處理，一般使用者無須輸入以下資訊。

| 平台 | 自動登錄位置 |
| --- | --- |
| Linux | `~/.config/{google-chrome,google-chrome-beta,google-chrome-canary,chromium}/NativeMessagingHosts/` 與 `~/.mozilla/native-messaging-hosts/` |
| macOS | `~/Library/Application Support/Google/Chrome{, Beta, Canary}/NativeMessagingHosts/`、`~/Library/Application Support/Chromium/NativeMessagingHosts/`、`~/Library/Application Support/Mozilla/NativeMessagingHosts/` |
| Windows | HKCU `Software\Google\Chrome\NativeMessagingHosts\<host>` 與 `Software\Mozilla\NativeMessagingHosts\<host>` 的預設值指向各自 manifest |

Windows 的 manifest 分別存於 `%LOCALAPPDATA%\Banana Hand\native-host-manifests\` 與 `%LOCALAPPDATA%\Mozilla\Firefox\NativeMessagingHosts\`。host binary 使用絕對路徑指向隨 App 分發的 sidecar；App 重新啟動時會更新登錄。

桌面與 host 間的 transport：Linux 使用 `$XDG_RUNTIME_DIR/banana-hand/` 的 Unix socket；macOS 使用使用者 cache 目錄；Windows 使用 per-app-pid named pipe。`bridge.json` 含每次啟動的 capability token，**不可貼到公開 issue**。正式 bridge 不使用 loopback TCP；測試用 WebView2 CDP port 是另一件事。

### 手動產生 manifest（僅開發用途）

以下兩個絕對路徑是示例，執行前改成實際 host 與輸出位置：

```sh
npm run native-host-manifest -- \
  --browser=firefox \
  --extension-id=bridge@banana-hand.dev \
  --host-path=/absolute/path/to/banana-hand-native-host \
  --out=/absolute/path/to/manifest.json
```

此 script **只產生檔案**，不會替你在各瀏覽器登錄。Chrome 使用 `--browser=chrome` 與實際 extension ID；`host-path`、`out` 都必須是絕對路徑。不要以某個手動 manifest 能生成，推論未列出的瀏覽器已受支援。

<a id="packaging"></a>

## 打包與發布

### 桌面 App

```sh
npm run tauri -- build
```

這只建置**目前執行平台**適用的 bundle，不會在一台機器上自動產生三平台安裝檔。`beforeBuildCommand` 依序執行 `npm run bundle-native-host` 與 `npm run build`。

sidecar script 也接受明確 target：

```sh
npm run bundle-native-host -- --target x86_64-pc-windows-msvc
```

這個例子仍需具備 Windows 建置工具。實際輸出為 `src-tauri/binaries/banana-hand-native-host-<target>[.exe]`；Tauri bundle 使用 [tauri.conf.json](../src-tauri/tauri.conf.json) 的 `externalBin` 納入 sidecar。

### 插件

```sh
npm run package-chromium-extension
npm run package-firefox-temporary-extension
npm run package-firefox-extension
```

| 指令／產物 | 用途與限制 |
| --- | --- |
| `package-chromium-extension` → `dist/chromium-extension/banana-hand-chromium-<版本>.zip` | 保留 manifest `key` 的本機載入版，ZIP 內有版本資料夾 |
| 同一指令 → `banana-hand-chrome-webstore-<版本>.zip` | 商店提交包，manifest 位於 ZIP 根目錄且不含 `key` |
| `package-firefox-temporary-extension` → temporary ZIP | 開發除錯使用；不代替正式 Firefox 安裝檔 |
| `package-firefox-extension` → `dist/firefox-extension/banana-hand-firefox-<版本>.xpi` | **未簽署**的本機包裝產物，不可冒充 Release 的 `-signed.xpi` |

AMO unlisted 簽署使用：

```sh
npm run sign-firefox-extension
```

需要事先透過安全方式提供 `web-ext` 所需的 AMO API 憑證。不要將金鑰寫入文件、原始碼或命令紀錄。CI 以 secrets 提供憑證，簽署失敗會中止發布；不要用未簽署包當作備援發行版。

### Release workflow

推送新的 `v*` tag 會執行 [build.yml](../.github/workflows/build.yml)：

1. Windows x64、macOS Apple Silicon、Linux x64 runner 各自建置。
2. 執行對應檢查，包含 Windows 登錄、release host 往返與實際安裝後 smoke。
3. 產生 Chrome 兩種 ZIP、Firefox temporary ZIP 與 AMO unlisted 已簽署 XPI。
4. 收集 `.exe`、`.dmg`、`.deb`、AppImage 與插件，發布同 tag 的 Preview Release。

macOS App 使用 **ad-hoc** 簽署，沒有 Developer ID／notarization；Windows 目前沒有 Authenticode 簽章。請區分產物生成、OS 簽章、插件 AMO 簽署與正式支援狀態。

[ADR 0002](adr/0002-store-extensions-manual-updates.md) 描述商店分發及插件商店更新的設計意圖，**不等同當前發行行為**：目前 Chrome 本機載入版與 Firefox unlisted 簽署版，均依[使用者教學](browser-extension-installation.md#updates)手動更新；AMO 簽署不代表商店上架或已配置自動更新。

<a id="testing"></a>

## 測試與驗證

先完成相依套件與 sidecar staging；在目前原生平台上執行：

```sh
npm run build
cargo build --locked -p banana-hand-native-host
cargo test --locked --workspace
npm run test:extensions
npx playwright install chromium
npm run test:ui
node --check extensions/chromium/background.js
node --check extensions/firefox/background.js
```

桌面 bridge 的真實 host 測試會啟動 `target/debug/banana-hand-native-host`，所以測試前需另外 `cargo build` 該 binary；不要把 test harness executable 當成真實 host。Playwright 在 Linux 可能另需系統相依套件，CI 使用 `npx playwright install --with-deps chromium`。

### Windows 原生驗證

以 [windows-bridge.yml](../.github/workflows/windows-bridge.yml) 為完整執行程序：

- 使用 matching target／profile 的真實 native host，執行 `windows_tests`，驗證閒置讀取時三輪 `prepare`／`prepared` 能往返。
- 編譯包含前端的測試專用桌面 App，透過 `TAURI_CONFIG` 僅對該建置開啟 WebView2 CDP。
- `scripts/verify-windows-browser.mjs` 啟動真實 Chromium 與插件，從 App 操作兩個分頁，確認各收到一次 trusted F8，保存 log 與桌面截圖。

**不要把任意正式安裝版 `.exe` 填進 browser smoke，就宣稱能執行同一種測試。** 此 script 需要 workflow 中的 CDP 測試建置；正式設定沒有開啟該除錯 port。

`scripts/verify-windows-startup.ps1` 則實際安裝 NSIS，驗證可見 App 視窗、程序存活、Chrome／Firefox registry 發現與 host `--self-check`，並保存檔案 SHA-256。它**沒有**驗證 browser workflow 或按鍵注入。

這些 Windows smoke 會啟動桌面程式、寫入該帳號的 host 登錄與測試設定，只能在可拋棄的 Windows 測試帳號／runner 執行，不要指向正在使用的個人環境。

測試失敗時，保留完整錯誤、退出碼、版本與對應 artifact；不要將重跑一次成功當成已修復，也不要未調查就歸因於 CI 環境。

<a id="verification"></a>

## 發布證據與限制

0.1.17 可核對的固定證據：

| 證據 | 驗證範圍 |
| --- | --- |
| [Release build 34188090969](https://github.com/botio/banana-hand/actions/runs/34188090969) | 三平台產物、Windows installed-startup／登錄／self-check、release host 往返 |
| [Chrome source smoke 34187904000](https://github.com/botio/banana-hand/actions/runs/34187904000) | Windows 測試建置、真實 Chromium 兩分頁，各一次 `key: F8`／`code: F8`／`isTrusted: true` |
| 使用者回報 | Windows 0.1.17 可正常使用；不等同所有環境的矩陣驗證 |

Linux X11 曾有本機驗證；Wayland 目前仍 fail-closed，實作計畫見 [Wayland 文件](wayland-remote-desktop-portal.md)。macOS 有建置與相關平台檢查，不能將 DMG 生成等同於完整按鍵送達。

正式支援宣稱依 [ADR 0003](adr/0003-release-evidence-matrix.md) 的 24-cell 證據矩陣；個別 CI 成功或使用者回報不取代完整矩陣。版本、commit、檔案雜湊、實際執行範圍需相互對應。
