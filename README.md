# Banana Hand

以 Tauri 2 + Rust 建立的桌面發送協調器。使用者在每次 App 啟動後，從已連線的 Chrome 或 Firefox WebExtension 選出兩個不同的 Browser Tab，再將快捷鍵庫中選定的單一組合按鍵依序做盡力原生發送。

## 下載與 Preview Release

每個推送的 `v*` tag 都會由 GitHub Actions 建立一個
[GitHub Preview Release](https://github.com/botio/banana-hand/releases)：內含 Windows
NSIS、macOS 未簽署 DMG、Linux `.deb`／AppImage，以及 Chrome extension 的 `.zip` 和
Firefox extension 的 `.xpi`。這些是可下載、可驗證的打包產物；**Preview 不表示任何
平台／browser 配對已得到正式支援宣稱**。正式支援前仍須完成
[ADR 0003 的 24-cell 實機發布證據矩陣](docs/adr/0003-release-evidence-matrix.md)。

Firefox `.xpi` 目前未經 AMO 簽署，只能在 Firefox Developer Edition、Nightly 或
ESR（停用 signature enforcement）安裝。

## 使用說明（給一般使用者）

Banana Hand 讓「一個快捷鍵」同時對兩個瀏覽器分頁做動作。第一次使用照下面四步走：

### 第 1 步：安裝桌面 App
從最新的 [GitHub Preview Release](https://github.com/botio/banana-hand/releases) 下載符合
作業系統的 desktop asset：
- **Windows**：執行 NSIS `.exe` 安裝程式。
- **macOS（僅 Apple Silicon）**：下載檔名含 `_aarch64.dmg` 的 Preview asset，把 `Banana Hand.app` 拖到「應用程式」。DMG 是 ad-hoc 簽章（無 Apple Developer ID、未 notarize），首次開啟會被 Gatekeeper 擋成「應用程式已損毀，無法開啟」；把 app 拖進 /Applications 後，在「終端機」清除一次隔離屬性即可正常開啟：`xattr -dr com.apple.quarantine "/Applications/Banana Hand.app"`（可能被要求允許 Terminal 變更）。ad-hoc／自簽簽章在近幾版 macOS 上無法產生「仍要開啟／Open Anyway」流程；若要移除這一次性的步驟需要 Apple Developer ID（$99/年），我們正在評估。
- **Linux**：安裝 `.deb`（或使用 AppImage）。

> App 靠「瀏覽器插件」看得到你的分頁。Preview Release 附帶 Chrome 的
> `banana-hand-chromium-<版本>.zip`，以及可在標準 Firefox 暫時載入的
> `banana-hand-firefox-temporary-<版本>.zip`。AMO unlisted-signed
> `banana-hand-firefox-<版本>.xpi` 只會在 release note 明示為「AMO-signed」時附帶。

**Chrome**
1. 從同版本 Preview Release 下載 `banana-hand-chromium-<版本>.zip` 並解壓。
2. 地址列輸入 `chrome://extensions`，按 Enter。
3. 右上角把「開發者模式」打開。
4. 按「載入未封裝的擴充功能」，選擇解壓後、含 `manifest.json` 的資料夾。
5. 清單出現「Banana Hand Browser Bridge」即成功。

**Firefox（目前可用的暫時載入）**
1. 從同版本 Preview Release 下載 `banana-hand-firefox-temporary-<版本>.zip` 並解壓。
2. 地址列輸入 `about:debugging#/runtime/this-firefox`，按 Enter。
3. 按「載入暫用附加元件…」（Load Temporary Add-on…），選擇解壓資料夾中的 `manifest.json`。
4. 清單出現「Banana Hand Browser Bridge」即成功；Firefox 每次重新啟動後要重做第 2–3 步。

> 若 release note 明示存在「AMO-signed」的 `.xpi`，可改到 `about:addons` → 右上齒輪 →「安裝附加元件…」永久安裝。未簽署 `.xpi` 不能在標準 Firefox 安裝。

### 第 3 步：讓 Browser 找到 Host（自動）

App 每次啟動時會自動把 native host 登錄到所有已知的 browser 目錄——
Chrome（stable／Beta／Canary）、Chromium、Firefox——不需要手動登錄、選 browser 或
貼任何值。App 上方的狀態列會顯示本次自動登錄的結果（各目錄路徑、host binary 是否存在）。

登錄時會**掃描本機已安裝的 Banana Hand extension**（各 Chrome profile 的
`Extensions/` 樹），把它們實際的 extension ID 一併寫進 manifest 的 allowlist
（除了 manifest key 固定的 ID 之外）。這樣即使是早期版本解壓安裝、ID 由路徑
衍生的 extension，Chrome 也會接受 manifest——不需要知道或抄寫任何 ID。

App 也會在啟動時對 sidecar 執行一次 `--self-check`（讀 bridge 設定、連 App 的
socket）；若失敗（例如 macOS Gatekeeper 把 host binary 杀掉），狀態列會顯示
`native host self-check：…`。

extension 連線後，App 會自動顯示可選 Browser Tab；若已載入舊版 extension，請到
`chrome://extensions`（或 `about:debugging`）對 Banana Hand 按「重新載入」一次。

extension 斷線後會自動重試（3 秒起、最多 30 秒間隔），**App 與 browser 的啟動順序不再重要**：
先開 App、後開 browser，或先開 browser、後開 App，extension 都會等到 native host 可用再完成
握手。Chrome 的 service worker 若被系統收回（idle 約 30 秒），extension 靠每分鐘一次
的 alarm 保證重試循環會重新啟動，不會卡死。App 重啟後 token 更換，extension 也會在數秒內
自動重新握手。

### 第 4 步：發送
1. 在「快捷鍵庫」按「新增快捷鍵」，填名稱；組合欄位**點一下再直接按鍵**（例如按住 `Ctrl+Shift` 再按 `K`），
   不要逐字輸入。裸按 `Esc` 取消錄製；裸 F 鍵（如 `F9`）可以單獨作為快捷鍵，其他主要按鍵必須搭配
   至少一個修飾鍵。
2. 快捷鍵庫中每一項右側有「×」，點一下即可刪除該快捷鍵。
3. 在「發送」面板的「目標 01」「目標 02」各選一個已連線的分頁（兩者必須不同）。
4. 選一個快捷鍵，按「發送快捷鍵」。
5. App 會依序對兩個分頁盡力送出，並在下方顯示結果。

### 常見問題
- **看不到我的分頁**：看 App 上方的連線狀態——它會指出卡在哪一級：
  - 「尚無 native host 連線」＝ extension 還沒握上：確認 App 正在執行、extension 已載入
    （載入後若 App 已開著，幾秒內會自動重試成功）。App 每次啟動已自動寫入所有
    Chrome 系與 Firefox 的 native messaging 目錄；若你用的是 Chrome Beta／Canary／Chromium，
    manifest 也已寫入它們各自的目錄。若 browser 是其他 Chromium 系（例如 Brave），
    其目錄不同，需要手動登錄（見「Browser extension 與 native host」）。
  - 狀態列若出現「最近一次 extension 回報」，那是 browser 對連不上 host 的原始診斷
    （例如 `Native messaging host not found`＝manifest 不在該 browser 的目錄、
    `Native host has exited`＝host binary 存在但啟動即退出）。如果從第一次就完全連不上
    （連回報都沒有），到 `chrome://extensions` → Banana Hand →「service worker」console
    看 `[banana-hand] native host connect failed: …` 的原始 lastError。
  - 狀態列若出現「native host self-check：failed…」，代表 App 自己都啟動不了 host
    binary（常見於 macOS 隔離屬性未清）；照提示處理後重開 App。
  - 「native host 已連線，但尚未收到 Browser Tab 快照」＝ extension 的 service worker 可能睡了：
    到 `chrome://extensions` 對 Banana Hand 按「重新載入」。
  - 「協定版本不符」＝ App 與 extension 版本不同步：下載同版本 release 的 extension 重新載入。
- **macOS 第一次連不上**：DMG 的 app 與 sidecar native host 都帶隔離屬性；若 browser 啟不起 native
  host，先執行 `xattr -dr com.apple.quarantine "/Applications/Banana Hand.app"`（見第 1 步），
  extension 會自動重試，不需要手動重載。
- **按下發送沒反應／顯示被拒**：看「發送」下方的結果。macOS／Windows 上，App 送出前會
  先等目標 browser 視窗**真的成為前景**（最多約 1.5 秒）；若超时被拒（「目標視窗沒有成為
  前景」），通常是視窗還在切換，再按一次即可。若提示 Accessibility 未授權，macOS 會直接
  彈出系統對話框（含跳轉系統設定的捷徑）；若清單裡有**多條**「Banana Hand」（舊版本留下
  的），先移除舊的、只保留目前版本並打勾。
- **快捷鍵沒送達**：這是「盡力發送」，App 只保證「有嘗試」，不保證網站收到（見下方「發送契約」）。
- **重啟 App 要重選分頁**：設計如此，分頁選取是 session-only（連線本身會自動恢復）。

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

App 啟動時的自動登錄（`native_host::auto_register`）會把同一份 manifest 寫入所有已知通道：
macOS 為 `~/Library/Google/Chrome{, Beta, Canary}/NativeMessagingHosts/`、
`~/Library/Application Support/Chromium/NativeMessagingHosts/`、
`~/Library/Application Support/Mozilla/NativeMessagingHosts/`；Linux 為
`~/.config/{google-chrome,google-chrome-beta,google-chrome-canary,chromium}/NativeMessagingHosts/`、
`~/.mozilla/native-messaging-hosts/`；Windows 的 Chrome 系共用
`%LOCALAPPDATA%\Banana Hand\native-host-manifests\` 並登記 HKCU registry 值，
Firefox 直接寫入 `%LOCALAPPDATA%\Mozilla\Firefox\NativeMessagingHosts\`。
manifest 中的 host 路徑預設為 App 執行檔的同層 sibling（sidecar），因此每次啟動都會
自動刷新——App 搬家之後也不需要手動重登錄。

產生 manifest 時必須提供實際、已發布的 extension identity 與絕對 host 路徑：

```sh
npm run native-host-manifest -- \
  --browser=firefox \
  --extension-id=bridge@banana-hand.dev \
  --host-path=/absolute/path/to/banana-hand-native-host \
  --out=/absolute/path/to/manifest.json
```

Chrome／Firefox 的 installer 登錄位置必須依官方文件寫入。Brave 等未列出的 Chromium 系 browser
不屬於自動登錄覆蓋範圍（其 native host lookup 無官方路徑保證），需要手動產生 manifest。

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
| Windows | named pipe（`%LOCALAPPDATA%\Banana Hand\runtime\`，per-pid pipe） | `SendInput` event stream（送前驗證前景窗口） | 實機待驗 |
| macOS Apple Silicon | Unix socket（`~/Library/Caches/Banana Hand/runtime`，0700） | CGEvent（Accessibility 提示 + 前景窗口驗證） | 實機待驗（ad-hoc DMG + xattr 清除隔離 + 授 Accessibility） |

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
  CGEvent 會先查 Accessibility；ad-hoc、未 notarize 的 DMG 首次開啟需先以
  `xattr -dr com.apple.quarantine` 清除隔離屬性、再授 Accessibility，仍待實機驗證。

因此 GitHub Release 目前一律標為 **Preview**：它提供版本化、可驗證的下載 artifact，
但沒有任何平台／browser 配對可被標示為已正式支援；發布前仍必須完成 ADR 0003 規定的
24-cell 發布證據矩陣。

## 打包與分發

native host 以 Tauri sidecar（`bundle.externalBin`）隨 App 一起打包：
`npm run bundle-native-host` 為目前 target（或 `--target <triple>`）編譯
`crates/native-host`，放到 `src-tauri/binaries/<name>-<triple>[.exe]`。
`register_native_host` 的預設 host 路徑是「執行檔同層」，正好落在 sidecar
位置，所以 installer 把兩者放進安裝目錄後，browser 不需額外地找到 host。

推送 `v*` tag 時，GitHub Actions 會原生建置三個 desktop target，下載 workflow artifacts，
再打包 Chrome extension `.zip` 與 Firefox unsigned `.xpi`，最後建立（或在 rerun 時更新）
同 tag 的 GitHub Preview Release。

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
