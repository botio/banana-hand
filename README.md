# Banana Hand

以 Tauri 2 + Rust 建立的桌面發送協調器。使用者在每次 App 啟動後，從已連線的 Chrome 或 Firefox WebExtension 選出兩個不同的 Browser Tab，再將快捷鍵庫中選定的單一組合按鍵依序做盡力原生發送。

## 使用說明（給一般使用者）

Banana Hand 讓「一個快捷鍵」同時對兩個瀏覽器分頁做動作。第一次使用照下面四步走：

### 第 1 步：安裝桌面 App
- **Windows**：執行 NSIS 安裝程式。
- **macOS**：把 DMG 裡的 `Banana Hand` 拖到「應用程式」。首次開啟會先被 Gatekeeper 擋，選「開啟」後再到「系統設定 → 隱私與安全性」再按「仍要開啟」，並授予「輔助功能」權限。
- **Linux**：安裝 `.deb`（或用 AppImage）。

### 第 2 步：加入瀏覽器插件
> App 靠「瀏覽器插件」看得到你的分頁。插件還沒上架商店：**Chrome** 可以用「未封裝」方式直接載入資料夾；**Firefox** 只接受打包好的 `.xpi`，而且**未簽名的 `.xpi` 只有 Firefox Developer Edition／Nightly／ESR 裝得到**（正式版會以「需要簽名」拒絕）——正式發行經 AMO 簽名後，任何 Firefox 都能直接裝。

**Chrome**
1. 地址列輸入 `chrome://extensions`，按 Enter。
2. 右上角把「開發者模式」打開。
3. 按「載入未封裝的擴充功能」，選到專案裡的 `extensions/chromium/` 資料夾。
4. 清單出現「Banana Hand」即成功。

**Firefox**
1. 先打包：跑 `npm run package-firefox-extension`，產生 `dist/firefox-extension/banana-hand-firefox-<版本>.xpi`。
2. （未簽名才需要）用 **Firefox Developer Edition／Nightly／ESR**，到 `about:config` 把 `xpinstall.signatures.required` 設成 `false`。
3. 地址列輸入 `about:addons` → 右上齒輪 →「安裝附加元件…」→ 選到第 1 步的 `.xpi`。
4. 出現「Banana Hand」即成功。

### 第 3 步：讓 Browser 找到 Host
1. 開啟 Banana Hand，到「讓 Browser 找到 Host」面板。
2. 選你的瀏覽器（`Chrome / Chromium` 或 `Firefox`）。
3. Chrome / Chromium 要填 **Extension ID**：回 `chrome://extensions` 點「Banana Hand」的「詳細資料」即可看到；Firefox 用固定 id，欄位留空。
4. 按「登錄 native host」，出現「已寫入 …」即成功。

### 第 4 步：發送
1. 在「快捷鍵庫」按「新增快捷鍵」，填名稱與組合（例如 `Ctrl+Shift+K`）。
2. 在「發送」面板的「目標 01」「目標 02」各選一個已連線的分頁（兩者必須不同）。
3. 選一個快捷鍵，按「發送快捷鍵」。
4. App 會依序對兩個分頁盡力送出，並在下方顯示結果。

### 常見問題
- **看不到我的分頁**：確認第 2 步插件已啟用、第 3 步已登錄，再重啟 App。
- **按下發送沒反應／顯示被拒**：看「發送」下方的結果；若提示權限不足，到系統設定授予輸入權限（macOS 為「輔助功能」）。
- **快捷鍵沒送達**：這是「盡力發送」，App 只保證「有嘗試」，不保證網站收到（見下方「發送契約」）。
- **重啟 App 要重選分頁**：設計如此，分頁選取是 session-only。

## 本機開發

```sh
npm install
cargo tauri dev
```

Linux 目前會在 `$XDG_RUNTIME_DIR/banana-hand/` 建立權限為使用者專屬的 Unix socket 與每次啟動更新的 capability token。native host 只能用該 token 連線；不使用 loopback TCP。

```sh
cargo build -p banana-hand-native-host
```

## Browser extension 與 native host

- Chromium MV3 source：`extensions/chromium/`。Chrome 使用；正式版從 Chrome Web Store 安裝。
- Firefox source：`extensions/firefox/`。其固定 Gecko Add-on ID 是 `bridge@banana-hand.dev`；正式版須由 AMO 簽署。
- desktop installer 應安裝 host binary 與各 browser 的 native messaging manifest，但不得旁載 extension。

產生 manifest 時必須提供實際、已發布的 extension identity 與絕對 host 路徑：

```sh
npm run native-host-manifest -- \
  --browser=firefox \
  --extension-id=bridge@banana-hand.dev \
  --host-path=/absolute/path/to/banana-hand-native-host \
  --out=/absolute/path/to/manifest.json
```

Chrome／Firefox 的 installer 登錄位置必須依官方文件寫入。Brave 不屬於首版支援範圍（其 native host lookup 無官方路徑保證）；此專案不假定 Chrome 的登錄位置對其他 Chromium 系 browser 有效。

App 內也可直接登錄 native host：在「讓 Browser 找到 Host」面板選 browser（Chromium 系需填 extension id，Firefox 使用固定 id），App 會寫入該 browser 的 native-messaging manifest——Linux 為 `~/.config/google-chrome/NativeMessagingHosts/` 或 `~/.mozilla/native-messaging-hosts/`，macOS 為 `~/Library/.../NativeMessagingHosts/`，Windows 則把 manifest 放在 `%LOCALAPPDATA%\Banana Hand\native-host-manifests\` 並登記 HKCU registry 值指向它。manifest 中的 host 路徑預設為 App 執行檔的同層 sibling，可另行指定。

## 發送契約

- Browser Tab、native host token、browser session nonce、目標選取、權限狀態、發送結果與 60 秒全域冷卻皆為 session-only。
- 僅快捷鍵庫（穩定 ID、名稱、單一組合按鍵、排序）存於 Tauri Store。
- 目標失效、host 斷線、權限遭拒或前景驗證失敗時 fail-closed；不重新搶焦點、不自動重試。
- `Attempted` 只代表 native input 已被嘗試，**不**代表 trusted event、送達、原子雙目標或 exactly-once。

## 平台輸入 adapter

各平台的 native host ↔ App 橋接與輸入注入，目前狀態如下（「已驗證」＝本開發機已跑通；
「實機待驗」＝source 已寫好，尚無該平台的真實機器證據）：

| 平台 | host ↔ App 橋接 | 輸入注入 | 狀態 |
|---|---|---|---|
| Linux X11 | Unix socket（`$XDG_RUNTIME_DIR/banana-hand/`，0700） | XTEST | 已驗證（本機） |
| Linux Wayland | 同上 | **fail-closed**：`PortalPermissionRequired` | spec-only，見下 |
| Windows | named pipe（`%LOCALAPPDATA%\Banana Hand\runtime\`，per-pid pipe） | `SendInput` event stream | 實機待驗 |
| macOS Apple Silicon | Unix socket（`~/Library/Caches/Banana Hand/runtime`，0700） | CGEvent（檢查 Accessibility） | 實機待驗（未簽署 DMG + Gatekeeper「Open Anyway」） |

- **Linux Wayland**：portal 注入是 spec-only。`XDG_SESSION_TYPE=wayland` 時維持
  fail-closed 閘門（`PortalPermissionRequired`），不嘗試靜默注入。精確的
  `org.freedesktop.portal.RemoteDesktop` v2 流程（`CreateSession` →
  `SelectDevices{types=1}` → `Start`（使用者授权）→ `NotifyKeyboardKeycode` →
  `Session.Close`）、keycode 映射、compositor/EIS 依賴與為何仍是 blocker，
  見 [docs/wayland-remote-desktop-portal.md](docs/wayland-remote-desktop-portal.md)。
- **Windows**：named-pipe bridge（App 端 server + native host 端 client）與
  `SendInput` 輸入串流皆已實作；per-app-pid pipe 名寫入 `bridge.json` 供 host 發現。
  FFI 已透過交叉編譯型別檢查（`banana-hand-native-host` 通過
  `cargo check --target x86_64-pc-windows-gnu`；App 的 `named_pipe`/`SendInput` FFI
  亦對 windows-sys 0.61 獨立通過型別檢查），但完整 App 建置仍需 mingw-w64
  工具鏈（本開發機無），故仍待實機 host 登錄與注入證據。
- **macOS**：Unix-socket bridge 已指向 macOS 使用者專屬 cache 目錄（0700），
  CGEvent 會先查 Accessibility；未簽署、未 notarize 的 DMG 需使用者 Gatekeeper
  「Open Anyway」+ 授 Accessibility，仍待實機驗證。

因此目前沒有任何平台／browser 配對可被標示為已發行；發布前必須完成 ADR 0003
規定的 24-cell 發布證據矩陣。

## 打包與分發

native host 以 Tauri sidecar（`bundle.externalBin`）隨 App 一起打包：
`npm run bundle-native-host` 為目前 target（或 `--target <triple>`）編譯
`crates/native-host`，放到 `src-tauri/binaries/<name>-<triple>[.exe]`。
`register_native_host` 的預設 host 路徑是「執行檔同層」，正好落在 sidecar
位置，所以 installer 把兩者放進安裝目錄後，browser 不需額外地找到 host。

各 artifact 的狀態（「已驗證」＝本開發機已產出並檢查；「實機待驗」＝
source 與打包邏輯就緒，尚無該平台真機證據）：

| Artifact | 產出方式 | 狀態 |
|---|---|---|
| Linux `.deb` | `npm run tauri -- build --bundles deb` | **已驗證（本機）**：tauri 原生 Rust 打包器產出；含 `usr/bin/banana-hand` 與 sidecar `usr/bin/banana-hand-native-host`（同層）、desktop 項 |
| Linux AppImage | `npm run tauri -- build --bundles appimage` | AppDir 階段已驗證正確（sidecar／desktop／`.DirIcon`／WebKit processes 齊備）；最後 `linuxdeploy` 打包在此機被擋——tauri（含最新版）捆綁的 linuxdeploy 帶的是 Binutils 2.35，無法解析本發行版（Arch）的 `.relr.dyn`（RELR）；在 Debian/Ubuntu 基底建置環境應可正常產出 |
| Windows NSIS | `cargo tauri build`（Windows） | 實機待驗：需 mingw-w64/MSVC 工具鏈；native host 的 FFI 已通過交叉編譯型別檢查 |
| macOS 未簽署 DMG | `cargo tauri build`（macOS） | 實機待驗：本機無 Apple Objective-C 工具鏈（App 的 CGEvent 無法在此型別檢查）；native host 的 Unix-socket 分支已通過 `aarch64-apple-darwin` 交叉檢查 |

## 檢查

```sh
cargo test --workspace
npm run build
node --check extensions/chromium/background.js
node --check extensions/firefox/background.js
```
