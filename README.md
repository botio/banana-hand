<img src="src-tauri/icons/icon.png" alt="Banana Hand 圖示" width="112" height="112">

# Banana Hand

**選好兩個瀏覽器分頁，按一次按鈕，把同一組快捷鍵依序送到兩個目標。**

Banana Hand 適合已經在網站或網頁工具中使用快捷鍵，希望省去來回切換分頁、重複按鍵的人。你可以替快捷鍵取名字、儲存在清單裡，再選擇這次要操作的兩個分頁。

**目前使用瀏覽器插件即可，不需要安裝桌面 App。** 插件版本 **0.1.24** 已取代舊的 Browser Bridge。完整步驟見 [插件使用說明](docs/browser-extension-installation.md)。

[下載插件](https://github.com/botio/banana-hand/releases/tag/v0.1.24) · [插件使用說明](docs/browser-extension-installation.md) · [隱私權政策](PRIVACY.md)

> 插件尚未上架 Chrome Web Store 或 Firefox 附加元件商店。Chrome 請載入未封裝項目；Firefox 0.1.24 是未簽署 XPI，需由 `about:debugging` 暫存載入，重啟後要再載入。傳送的是合成按鍵事件，網站可能忽略。交易用途請先用模擬帳戶。

## 目錄

- [先了解：它會做什麼？](#about)
- [下載：我應該選哪個檔案？](#downloads)
- [第一次安裝桌面 App](#desktop-install)
- [安裝 Chrome 或 Firefox 插件](#extensions)
- [確認已經連線](#connection)
- [第一次新增及發送快捷鍵](#first-dispatch)
- [看懂發送結果與冷卻時間](#results)
- [日常使用與更新](#updates)
- [常見問題](#faq)
- [回報問題與隱私](#support)
- [開發者文件與驗證紀錄](#development)

<a id="about"></a>

## 先了解：它會做什麼？

### 你需要準備

- Google Chrome 118+ 或 Firefox 140+。
- 兩個已開啟的一般網頁分頁。
- 一組已在目標網站手動測試過的快捷鍵。

### 它能做的事

- 在插件控制頁儲存並命名快捷鍵。
- 選擇兩個不同分頁，依序傳送同一組按鍵。
- 顯示每個分頁的傳送結果，以及 15 秒冷卻倒數。

### 使用前務必知道

- **不需要桌面 App，也不使用 debugger。**
- **兩個目標必須是不同分頁。**
- **不是背景靜默操作。** 傳送時會切換到目標分頁；期間不要改焦點。
- **不是巨集。** 一次只送一組按鍵，不包含文字、滑鼠或延遲。
- **不會替網站新增快捷鍵。** 網站若不接受這組按鍵，插件不能保證有動作。
- **兩個目標不是原子完成。** 可能只有一邊執行。不要因此直接重送兩個。
- **接受傳送後有 15 秒全域冷卻。** 換快捷鍵或換分頁也不能跳過。
- **按下傳送時，瀏覽器只詢問那兩個分頁所在網站的權限。** 不需要事先設定網站清單。

第一次請用不會刪除資料、付款、送出表單或造成其他不可逆結果的操作測試。

<a id="downloads"></a>

## 下載：我應該選哪個檔案？

### 1. 開啟發布頁面

前往 **[Banana Hand Releases](https://github.com/botio/banana-hand/releases)**，找到要安裝的版本，再展開該版本下方的 **Assets**（下載檔案清單）。

- 插件請用 **[v0.1.24](https://github.com/botio/banana-hand/releases/tag/v0.1.24)**。不要選 `Source code`。
- 不要安裝 0.1.21 或更早的 Browser Bridge，也不要混用 `banana-hand-chrome-webstore-*.zip`。Web Store 壓縮檔只供商店提交。

| 瀏覽器 | 檔案 | 安裝方式 |
| --- | --- | --- |
| Chrome | [banana-hand-chromium-0.1.24.zip](https://github.com/botio/banana-hand/releases/download/v0.1.24/banana-hand-chromium-0.1.24.zip) | 解壓縮後，在 `chrome://extensions` 載入未封裝項目 |
| Firefox | [banana-hand-firefox-0.1.24.xpi](https://github.com/botio/banana-hand/releases/download/v0.1.24/banana-hand-firefox-0.1.24.xpi) | 在 `about:debugging` 載入暫存附加元件；尚未上架，重啟後需再載入 |

更早的桌面 App 安裝檔仍在舊 Release，**不是使用目前插件的必要步驟**。

<a id="desktop-install"></a>

## 舊版桌面 App（目前插件不需要）

下面的 Windows、macOS、Linux 步驟只適用於 0.1.21 以前的桌面版。使用 0.1.24 插件時，請跳過這整節，直接看[插件使用說明](docs/browser-extension-installation.md)。

### Windows

1. 下載檔名以 `_x64-setup.exe` 結尾的安裝程式。
2. 如果已有 Banana Hand 正在執行，先關閉舊的 App 視窗。
3. 開啟下載的 `.exe`，依照安裝精靈完成安裝。一般情況使用預設選項即可，不需要自行修改登錄檔。
4. 安裝完成後，從開始功能表或安裝程式提供的捷徑開啟 **Banana Hand**。
5. 看到「快捷鍵庫」與「發送」面板後，保持 App 開著，再安裝瀏覽器插件。

**第一次下載或安裝被 Windows 提醒時：**

- 目前 Windows 安裝程式沒有 Authenticode 發行者簽章，可能出現不明發行者或信譽提示。
- 先確認來源是本專案的 GitHub Release、檔名及版本正確，再決定是否繼續。
- 若 Windows 顯示 SmartScreen 並提供「其他資訊／More info」與「仍要執行／Run anyway」，只有在你確認來源且信任程式時才使用這些選項。
- 如果公司或學校政策禁止執行，請交由管理員處理；不要停用防毒、SmartScreen 或安全政策。

安裝檔內含 WebView2 Runtime 離線安裝程式，檔案較大是正常的。**不需要另外安裝 Node.js、Rust 或所謂的 host 套件。**

### macOS（Apple Silicon）

1. 在 Apple 選單的「關於這台 Mac」確認是 M 系列晶片；目前提供的是 Apple Silicon 版本。
2. 下載並開啟 `_aarch64.dmg`。
3. 把 **Banana Hand.app** 拖到「應用程式」，不要一直從 DMG 裡執行。
4. 從「應用程式」開啟 Banana Hand。
5. 如系統要求「輔助使用／Accessibility」權限，前往「系統設定 → 隱私權與安全性 → 輔助使用」，允許目前安裝的 Banana Hand，必要時重新開啟 App。
6. 保持 App 開著，再安裝瀏覽器插件。

目前 DMG 內的 App 採 ad-hoc 簽署，沒有 Apple Developer ID／notarization，因此 macOS 可能攔截首次啟動。

**只在確認下載來源可信，而且已經把 App 放進「應用程式」後**，如果仍因隔離屬性無法開啟 App 或它內含的連線程式，可在「終端機」執行以下只針對 Banana Hand 的指令：

```sh
xattr -dr com.apple.quarantine "/Applications/Banana Hand.app"
```

然後重新開啟 App。這不是關閉整台電腦的 Gatekeeper；**不要改成移除整個「應用程式」資料夾的隔離屬性**。如果你不確定安全提示的原因，先保留完整提示並回報，不要盲目執行繞過指令。

### Linux

先確認目前桌面工作階段是 **X11**。可查看系統登入選項，或在終端機執行：

```sh
echo "$XDG_SESSION_TYPE"
```

輸出 `wayland` 時，這個版本會拒絕發送；需改用系統提供的 X11 工作階段。這不是插件安裝錯誤。

**Debian／Ubuntu 類系統**：用軟體安裝工具開啟 `.deb`，或在下載檔案所在目錄執行（下例為 0.1.17）：

```sh
sudo apt install ./Banana.Hand_0.1.17_amd64.deb
```

**AppImage**：在檔案管理員的檔案內容／權限中允許執行，再開啟；也可以在下載目錄執行：

```sh
chmod +x Banana.Hand_0.1.17_amd64.AppImage
./Banana.Hand_0.1.17_amd64.AppImage
```

AppImage 是否需要額外的系統元件，取決於 Linux 發行版。若無法啟動，請保留終端機錯誤，不要把桌面 App 改成用 `sudo` 執行，也不要先改瀏覽器安全設定。

<a id="extensions"></a>

## 安裝 Chrome 或 Firefox 插件

完整步驟在 [插件使用說明](docs/browser-extension-installation.md)。

| Chrome | Firefox |
| --- | --- |
| 下載並解壓縮 `banana-hand-chromium-0.1.24.zip` | 下載 `banana-hand-firefox-0.1.24.xpi`，不要解壓縮 |
| `chrome://extensions` → 開發人員模式 → 載入未封裝項目 | `about:debugging#/runtime/this-firefox` → 載入暫存附加元件 |
| 選擇直接含 `manifest.json` 的資料夾 | 選擇 XPI。尚未上架，重啟後要再載入 |
| 點香蕉圖示開啟控制頁 | 點香蕉圖示開啟控制頁 |

控制頁會顯示「擴充功能已連線」。看不到分頁時，先按「重新整理」，並確認網頁開在安裝插件的同一個瀏覽器設定檔。

## 第一次新增及發送快捷鍵

目前操作在插件控制頁，不在桌面 App。點工具列香蕉圖示開啟後：

1. 在「按鍵設定」輸入名稱與組合，例如 `Shift+B`，或按「擷取按鍵」後直接按下組合，再按「儲存快捷鍵」。
2. 在「選擇兩個分頁」選不同的分頁 A、B，按「儲存目的地」。
3. 在右側選擇要傳送的快捷鍵，核對摘要，再按「傳送至兩個分頁」。
4. 允許瀏覽器詢問的那兩個網站權限。傳送時不要切換焦點。
5. 分別檢查兩個網站的實際結果。插件的「已嘗試傳送」不是執行成功收據。

可儲存的按鍵包含 `A`–`Z`、`0`–`9`、`F1`–`F24`，以及 `Esc`、`Enter`、`Tab`、`Space`、方向鍵等，可搭配 `Ctrl`、`Shift`、`Alt`、`Meta`。一次只能有一組按鍵，不能錄成巨集。

要刪除快捷鍵，按該列的「刪除」。按鈕是灰色時，先檢查是否已儲存兩個不同分頁、是否已選快捷鍵、是否仍在冷卻。不要連點。

<a id="results"></a>

## 看懂發送結果與冷卻時間

| 你看到的訊息 | 代表什麼 | 接下來怎麼做 |
| --- | --- | --- |
| 「已嘗試傳送」 | 插件已送出合成按鍵事件，不是網站執行成功的收據 | 分別檢查兩個網站的實際結果 |
| `partial`／部分目標已嘗試 | 這次沒有完整完成；第一個目標可能已經執行 | **先核對兩邊，不要直接重送整組** |
| 「目標視窗沒有成為前景」 | 無法確認預期瀏覽器在前景，安全檢查拒絕繼續 | 確認版本、視窗、彈出對話框與焦點；不是把檢查關掉 |
| 「目標前景驗證逾時」 | 等待目標準備回覆時逾時，不能單靠這句斷定原因 | 先檢查連線與版本，保留完整訊息以便排查 |
| 「冷卻 … 秒」 | 15 秒全域冷卻尚未結束 | 等倒數結束，不用重裝插件 |
| 目標失效／已斷線 | 分頁關閉、移動或連線狀態改變，原選取可能已無效 | 重新確認分頁清單，再選取兩個目標 |

冷卻在接受這次傳送時就開始，即使後面沒有全部成功也不縮短。這是為了降低重複送出風險。

對付款、刪除、送出表單等不可逆操作，不要只看 App 的「已嘗試發送」就當作兩邊都成功，更不要把重送當成一般排錯步驟。

<a id="updates"></a>

## 日常使用與更新

### 每次開啟時

- 開啟瀏覽器與插件控制頁，重新選擇兩個分頁。快捷鍵會保留，目標分頁不會。
- 傳送期間不要切換程式、關閉目標分頁或操作其他快捷鍵。
- Chrome 的解壓縮資料夾要留在原位。Firefox 暫存附加元件在瀏覽器重啟後要重新載入。

### 更新插件

從 [Releases](https://github.com/botio/banana-hand/releases) 下載新版插件，依[插件使用說明](docs/browser-extension-installation.md)覆蓋 Chrome 資料夾或重新載入 Firefox XPI。不要用「解除安裝並清除瀏覽器資料」當成更新。

<a id="faq"></a>

## 常見問題

### 控制頁看不到分頁？

確認插件已啟用，而且網頁開在安裝插件的同一個瀏覽器設定檔。Chrome 的「個人」和「工作」是不同設定檔。按控制頁的「重新整理」。不要用無痕視窗或瀏覽器內部頁當第一個目標。

### 按插件圖示沒有反應，是壞了嗎？

0.1.24 的圖示應開啟控制頁。若沒有香蕉圖示，先在瀏覽器的擴充功能選單把它釘選；Firefox 請確認載入的是 0.1.24，不是較舊的暫存包。

### Chrome 找不到「載入未封裝項目」？

確認開的是 `chrome://extensions`，且已開啟「開發人員模式」。如果開關不存在、無法啟用，或瀏覽器顯示由組織管理，可能受政策限制；請向管理員確認，不要嘗試繞過。

### Windows 又出現「目前前景：banana-hand.exe」？

先確認 App 使用 **0.1.17 或更新版本**，並重新啟動更新後的程式。接著確認 Chrome 視窗可正常操作，沒有阻擋中的對話框，發送期間沒有自行切換程式。

若仍出現同樣訊息，請回報完整錯誤與版本。不要把它當成「多按幾次一定會好」，也不要為了送出按鍵而停用前景檢查。

### 顯示「已嘗試發送」，但網頁沒變化？

先直接到該網頁，手動按一次同樣的組合，確認網站本來就會處理。再檢查網頁內的焦點位置，例如是否停在網址列、輸入框、彈出對話框，或快捷鍵被瀏覽器本身攔截。

如果手動可用、透過 App 不可用，保留使用的組合與兩個目標的操作結果回報；**不要用連續重送來測試可能重複執行的動作**。

### Linux Wayland 顯示權限或 portal 相關拒絕訊息？

目前 Wayland 發送尚未實作完成，不是授予一個權限就能修好。請改用可用的 X11 工作階段，或等待後續版本。

### 我需要輸入 extension ID、修改登錄檔或另外安裝 native host 嗎？

一般使用者不需要。這些是 App 自動處理或開發者除錯的細節。請先使用正確的 App 和插件檔案，遇到問題再依教學排查。

### 想停用或移除？

在瀏覽器的擴充功能／附加元件管理頁停用或移除 Banana Hand Browser Bridge；要移除桌面 App，使用作業系統正常的解除安裝方式。

**移除插件、移除 App、清除快捷鍵資料是不同事情。** 詳細順序與注意事項見[移除教學](docs/browser-extension-installation.md#removal)。

<a id="support"></a>

## 回報問題與隱私

遇到問題，可在 [GitHub Issues](https://github.com/botio/banana-hand/issues) 回報。請提供：

1. 作業系統與版本，例如 Windows 的版本及 x64／ARM 架構。
2. App 版本、瀏覽器名稱與版本、插件版本。
3. 問題出現在安裝、連線、選取分頁，還是發送階段。
4. App 的完整錯誤文字，必要時附遮蔽過的截圖。
5. 使用的快捷鍵，以及兩個目標各自實際發生什麼事；是否在同一瀏覽器、同一設定檔。

**GitHub Issues 是公開的。** 請遮蔽私人分頁標題、完整網址中的識別碼、帳號、密碼、權杖與其他敏感資料；不需要上傳整個瀏覽器設定檔或完整連線設定。

插件會處理已開啟分頁的標題、網址及識別資訊，讓本機 App 列出可選目標；不限於你最後選中的兩個分頁。這些資料不會由插件自動傳送到開發者伺服器。詳見[隱私權政策](PRIVACY.md)。

<a id="development"></a>

## 開發者文件與驗證紀錄

一般使用者不需要執行任何開發指令。想自行修改或編譯，請看[開發與打包說明](docs/development.md)。

目前可核對的 0.1.17 紀錄：

- [三平台發布與 Windows 安裝後檢查](https://github.com/botio/banana-hand/actions/runs/34188090969)：Windows、macOS、Linux 建置完成；Windows 已安裝程式通過啟動、登錄與連線助手自我檢查。
- [Windows 真實 Chromium 兩分頁測試](https://github.com/botio/banana-hand/actions/runs/34187904000)：每個分頁收到一次 `key: F8`、`code: F8`、`isTrusted: true`；包含實際桌面截圖。此測試使用測試專用 CDP 建置，不等同於在每一份已安裝發行版、每一種瀏覽器配對上都完成驗證。
- Windows 0.1.17 已有使用者正常使用的回報。個別成功結果不代表所有網站或電腦都相容；完整支援證據規則見 [ADR 0003](docs/adr/0003-release-evidence-matrix.md)。

[網域說明](CONTEXT.md) · [Wayland 技術限制](docs/wayland-remote-desktop-portal.md) · [發布檔案](https://github.com/botio/banana-hand/releases)
