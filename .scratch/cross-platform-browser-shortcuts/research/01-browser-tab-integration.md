# Browser Tab 跨瀏覽器整合研究

> 範圍：Windows x64、macOS Apple Silicon、Linux x64 上的 Chrome、Firefox、Brave；來源限瀏覽器廠商文件與 Web 規格。本文區分 WebExtensions 可保證的控制面，以及瀏覽器文件沒有保證的 OS 輸入面。

## 結論

採用**每個瀏覽器安裝一個 WebExtension，並由同一個隨桌面 App 安裝的 Native Messaging host 匯流**。Extension 是唯一可取得其所屬瀏覽器 profile 內分頁資料、啟用分頁、並回報生命週期變更的受支援途徑；Native Messaging 只是一條受 allowlist 限制的 stdio IPC，不是鍵盤自動化 API。

此組合可達成問題 1、2 與「失效即 fail-closed」的問題 4，但**不能以 WebExtensions 或 Native Messaging 保證問題 3 的兩個目標收到瀏覽器／網頁信任的同一組快捷鍵**。`commands` 只能登錄並接收「本 extension」的命令；不存在可對任意 tab 派送 browser-trusted chord 的 Tabs/Windows/Native Messaging API。網頁端 `new KeyboardEvent()` + `dispatchEvent()` 是 synthetic DOM event，必為 `isTrusted === false`；不能替代真實鍵盤、瀏覽器快捷鍵或 extension command。

若產品仍要派送快捷鍵，Native host 必須另實作 OS 級「依序」前景切換與輸入注入（每個目標各一次），並在每一次注入前重新驗證。這已在 WebExtensions 契約之外：本研究的官方瀏覽器文件**不保證**任何 Windows/macOS/Linux 注入 API 能產生 browser-/page-trusted event、原子地固定前景焦點，或讓兩個 tab 同時接收鍵盤。因此首版規格必須把它表述為「盡力依序派送」，不得宣稱 trusted/atomic/exactly-once 保證。

## 建議的安裝與橋接模型

1. 發行三個 browser installation target：Chrome、Firefox、Brave 各安裝一個 extension（Chrome/Brave 可共用 Chromium MV3 程式碼，但仍以實際安裝的 extension ID/origin 為準）。桌面 App 的 installer 另外安裝同一 native host binary 與各瀏覽器所需 host manifest。
2. 每個 extension 宣告 `nativeMessaging`，background/service worker 以 `connectNative()` 建立長連線；content script 不可直接連 native host。host manifest 必須列出精確 extension identity，不能使用 wildcard：Chrome/Chromium 用 `allowed_origins` 的 `chrome-extension://<id>/`；Firefox 用 `allowed_extensions` 的 Add-on ID。Firefox manifest 要設定固定 Gecko Add-on ID；該 key 不可直接用於 Chrome manifest，故需要 browser-specific manifest/package。
3. Extension 回報可選 tab 的 `{ browser, profile/install-instance nonce, windowId, tabId, generation }` 與顯示資料。`tabId`/`windowId` 僅在各自 browser session 內唯一；重啟後可重用，故不得作為跨 session 或跨瀏覽器的永久 ID。`title`、`url`、`favIconUrl` 需要 `tabs` 或相符 host permission；不要求這些權限時仍可用 ID 做選取。
4. Desktop App 不直接枚舉 browser process/tab；它只保存 extension 的 session-scoped選取值。派送前送 `prepare(target, generation)` 到每個 extension；extension 以 `tabs.get()`/`tabs.query()`、`windows.get()` 與目前 session nonce 驗證 tab/window 仍存在、tab 仍屬指定 window，才回覆 ready。任何斷線、查無 ID、generation 不合、事件通知已移除/替換都拒絕。
5. 對每個 ready target：extension 先 `tabs.update(tabId, { active: true })`，再 `windows.update(windowId, { focused: true })`；完成後仍只能表示 API 已解決，**不是 OS 級「鍵盤輸入已可安全到達文件」的保證**。若實作 OS 輸入後端，host 在每一個 target 前都要重跑上述驗證，任一步失敗即停止、回報未派送，且不可把第二個 target 視為已完成。

## 逐題答案

### 1. 可否枚舉並唯一識別使用者可選取的 tab？

**可以，但唯一性僅限 extension 所在的 browser session。**

| Browser | 受支援能力 | 權限與識別限制 |
| --- | --- | --- |
| Chrome | `chrome.tabs.query()` 可列舉 tab，`chrome.windows.getAll({ populate: true })` 可同時取得 window 與 tab；`Tab.id` 供後續 `tabs.get()`/`tabs.update()` 使用。 | Chrome 明確規定 tab ID 僅在 browser session 內唯一；`tabs` 或相符 host permission 才能取得 title/URL/favicon。 [Chrome Tabs](https://developer.chrome.com/docs/extensions/reference/api/tabs)；[Chrome Windows](https://developer.chrome.com/docs/extensions/reference/api/windows) |
| Firefox | `browser.tabs.query()` 或 `browser.windows.getAll({ populate: true })` 可列舉；Firefox 明確規定 tab ID 僅在一個 browser session 的單一 tab 中唯一，重啟可能重用。 | `tabs` 或相符 host permission 才可讀 title/URL/favicon；`activeTab` 僅在顯式使用者動作後暫時給目前 active tab。 [Firefox Tabs](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/API/tabs)；[tabs.query](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/API/tabs/query)；[windows.getAll](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/API/windows/getAll) |
| Brave | Brave 官方說明 Chrome Web Store extension 可在 Brave 使用，且 MV3 extension「just as they do in Chrome」。因此 Chromium Tabs/Windows API 是合理的 extension 控制面。 | Brave 的頁面不是逐 API/逐平台相容性保證；不可據此宣稱每個 Chromium edge case、native-host location 或 ID 行為已由 Brave 保證。 [Brave extension support](https://brave.com/learn/using-chrome-extensions-in-brave/) |

跨瀏覽器唯一鍵必須含 browser identity 與本次 extension connection/session nonce；僅存 `{windowId, tabId}` 會在不同 browser、profile 或重啟後碰撞。

### 2. 可否把指定 tab 變成可接收輸入的目標？

**可啟用 tab 並要求 browser window 取焦點；不可由 browser API 證明文件已取得 OS 真實鍵盤焦點。**

* Chrome/Chromium：`tabs.update()` 提供 `active`，`windows.update()` 提供 `focused`；Windows API 文件也提醒 extension 所謂 current window 可能不是 topmost/focused window，應指定明確 `windowId`。 [Chrome Tabs](https://developer.chrome.com/docs/extensions/reference/api/tabs#method-update)；[Chrome Windows](https://developer.chrome.com/docs/extensions/reference/api/windows#method-update)
* Firefox：`tabs.update(tabId, { active: true })` **不影響 window 是否 focused**；必須再以 `windows.update(windowId, { focused: true })` bring to front。 [Firefox tabs.update](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/API/tabs/update)；[Firefox windows.update](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/API/windows/update)
* Brave：依官方 Chrome MV3 相容性聲明，可使用上述 Chromium 控制面；但 Brave 沒有在該聲明中保證 OS focus policy 或 native input delivery。 [Brave extension support](https://brave.com/learn/using-chrome-extensions-in-brave/)

這些 API 的 Promise 成功不是「可收到下一個 OS key event」的原子確認；使用者或 OS 可以在完成後改變焦點，且兩個 browser window 不可能同時是實體 keyboard focus 目標。

### 3. 可否對兩個選取目標派送同一 shortcut chord？可否是 trusted？

**不能由 WebExtensions/Native Messaging 直接做到；也不能保證 trusted。**

* Chrome `commands` 只讓 extension 在 manifest 宣告其自身 command 的 suggested shortcut，並由 `onCommand` 接收；保留的 OS/Chrome shortcut 優先且不可覆寫。它不是「向指定 tab 注入任意 chord」API。 [Chrome Commands](https://developer.chrome.com/docs/extensions/reference/api/commands)
* Firefox `commands` 同樣是 extension command 註冊/接收；已被 browser 或既有 add-on 使用的組合不能覆寫，handler 不會被呼叫。 [Firefox commands manifest](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/manifest.json/commands)
* Chrome 與 Firefox 的 Native Messaging 都只定義 extension↔host 的 JSON/stdio 訊息通道，沒有 keyboard injection method。Chrome 還明定 Native Messaging 不可直接在 content script 使用。 [Chrome Native Messaging](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging)；[Firefox Native Messaging](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Native_messaging)
* Chrome 的另一條官方 automation surface 是：extension 宣告高權限 `debugger`、attach target 後可送 CDP `Input.dispatchKeyEvent`。這不是 Tabs/Native Messaging，且文件沒有承諾其 event 的 `isTrusted`、user activation、OS 實體焦點或跨平台結果；不可作為 trusted physical input 的保證。Brave 只承諾「nearly all Chromium-compatible extensions」，沒有為 Debugger/CDP Input 提供相同保證；Firefox 本研究沒有找到與此相同、可由一般安裝 extension 使用的官方輸入派送契約。 [Chrome Debugger](https://developer.chrome.com/docs/extensions/reference/api/debugger)；[CDP Input](https://chromedevtools.github.io/devtools-protocol/tot/Input/)；[Brave extension support](https://support.brave.com/hc/en-us/articles/360017909112-How-can-I-add-extensions-to-Brave)
* WHATWG DOM 規定程式建立的 event 初始 `isTrusted` 為 `false`；只有 user agent dispatch 的 event 才是 true。故 content script 的 `new KeyboardEvent()`/`dispatchEvent()` 只能通知可監聽 DOM listener，不能偽造 browser/page-trusted shortcut。 [DOM `isTrusted`](https://dom.spec.whatwg.org/#dom-event-istrusted)；[W3C UI Events](https://www.w3.org/TR/uievents/)

因此 host 的 OS 輸入後端若存在，只能實作 `target A: focus → inject → observe/abort`，再做 target B；它不是同時派送，亦不是任何上述 browser 文件承諾的 trusted shortcut。OS 保留鍵、browser 保留鍵、使用者改寫 command、前景焦點競爭及限制 URL 都可能改變結果。這些限制在 Windows x64、macOS Apple Silicon、Linux x64 都必須按實機驗證，而非由 extension API 推定。

### 4. 目標失效時可否安全拒絕派送？

**可 fail-closed 拒絕已知失效或不一致的目標；不能取得無競態的跨 browser delivery 保證。**

每次派送前與切換每一個 target 前，extension 必須重新取得 tab/window 並比對 session nonce、`tabId`、`windowId`、預期 browser/profile；查無、已替換、失去 native port 或不再隸屬指定 window 即拒絕。並維護 `tabs.onRemoved`、`tabs.onReplaced`、`tabs.onDetached`/`onAttached`、`windows.onRemoved`/`onFocusChanged` 等事件使選取即時失效。Chrome 的事件清單見 [Tabs](https://developer.chrome.com/docs/extensions/reference/api/tabs)、[Windows](https://developer.chrome.com/docs/extensions/reference/api/windows)；Firefox 的 tabs API 亦提供對應 lifecycle events。 [Firefox Tabs](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/API/tabs)

但 check、focus、OS injection 之間沒有跨 browser 的 transaction；使用者可在最後檢查後關閉/移動 tab 或改變焦點。故正確保證是「無法重新驗證時不派送」，不是「一旦 ready 就必然送到原 tab」。

## Native Messaging、權限與發行條件

| Browser | Extension/host 授權與安裝 | 平台差異 |
| --- | --- | --- |
| Chrome | manifest 宣告 `nativeMessaging`；host manifest `allowed_origins` 必須列精確 `chrome-extension://<id>/`，禁止 wildcard。host 由 browser 在獨立 process 啟動並以 stdio 交談；browser 不安裝 host。 [Chrome Native Messaging](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging) | Windows：installer 寫 `HKCU`/`HKLM\\Software\\Google\\Chrome\\NativeMessagingHosts\\<name>`；Chrome 先查 32-bit registry 再查 64-bit。macOS/Linux：manifest 與 binary path 必為 absolute，Chrome 文件列 system/user locations。 |
| Firefox | manifest 宣告 `nativeMessaging`（可為 optional）；host `allowed_extensions` 列精確 Add-on ID，且 native host 不由 browser 安裝/管理。Firefox extension 需固定 Gecko ID 才能 allowlist。Release/Beta 的一般發行需 Mozilla signing；Temporary Add-on 重啟即移除。 [Firefox Native Messaging](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Native_messaging)；[Native manifests](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Native_manifests)；[Firefox signing](https://extensionworkshop.com/documentation/publish/signing-and-distribution-overview/) | Windows：`HKCU`/`HKLM\\Software\\Mozilla\\NativeMessagingHosts\\<name>`，Firefox 64+ 先查 Wow6432Node 再 native registry view。macOS：`/Library/Application Support/Mozilla/NativeMessagingHosts/` 或使用者 Library；Linux：`/usr/lib/mozilla/`、`/usr/lib64/mozilla/` 或 `~/.mozilla/native-messaging-hosts/`。macOS/Linux binary path 必為 absolute。 |
| Brave | Brave 支援從 Chrome Web Store 安裝 extension，安裝時使用者批准資料/permission access，並宣稱 MV3 如 Chrome 般運作。 [Brave extension support](https://brave.com/learn/using-chrome-extensions-in-brave/) | 本研究未找到 Brave 官方 Native Messaging host manifest/registry/path 文件。因此不得假定 Chrome 的 Google Chrome locations 或 registry keys 對 Brave 有效；Brave host discovery 必須列為發布前 blocker，向 Brave 官方確認或用三平台實機驗證後才寫入 installer。 |

## 目標平台矩陣與未保證事項

| 平台 | 文件化的 browser-side 條件 | 不可從本研究來源保證的事項 |
| --- | --- | --- |
| Windows x64 | Chrome 的 host lookup 先 32-bit 再 64-bit registry；Firefox 64+ 先 Wow6432Node 再 native registry view。兩者 host path 可相對 manifest。 | 文件未指定 host EXE 的 x64/ARM ABI 要求、能否前景取焦點、或 OS 注入是否會形成 trusted browser event；Brave host registration 未文件化。 |
| macOS Apple Silicon | Chrome 與 Firefox 均有 system/user NativeMessagingHosts location，host path 必為 absolute；extension commands 有 `mac` platform selector，且 Chrome/Firefox 都將 `Ctrl` 以 Command 語意處理，若需要實體 Control 要用 `MacCtrl`。 [Chrome Commands](https://developer.chrome.com/docs/extensions/reference/api/commands)；[Firefox commands](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/manifest.json/commands) | 兩家 browser 文件僅寫「macOS」，沒有 Apple Silicon/ARM64 或 Universal host、簽署/notarization、Accessibility/TCC、或注入事件 trust 的 API 保證。必須由桌面發行與實機驗證承擔。 |
| Linux x64 | Chrome 文件列 `/etc/opt/chrome/native-messaging-hosts` 與 `~/.config/google-chrome/NativeMessagingHosts`；Firefox 文件列 `/usr/lib/mozilla`、`/usr/lib64/mozilla` 與 `~/.mozilla/native-messaging-hosts`；host path 必為 absolute。 | 沒有文件保證特定發行版、desktop environment、X11/Wayland session 或 input-injection backend 的行為；Brave host location 未文件化。 |

上述 OS/API 文件也沒有按 CPU architecture 為 Tabs、Windows、Commands 或 event-trust 發布差異。因此「Windows x64、macOS Apple Silicon、Linux x64 都可包裝 native host」是產品發行工作，不是這些 API 文件提供的跨架構相容性證明。

## 後續規格約束

* 將「可選 Browser Tab」定義為 session-scoped capability，不是跨啟動永久實體。
* 介面必須揭露 Chrome、Firefox、Brave 各自的 extension 安裝與權限（`tabs` 若要顯示 title/URL；`nativeMessaging` 必要），且把 native host 安裝視為 desktop installer 職責。
* 派送狀態至少要有 `rejected-stale`、`rejected-disconnected`、`focus-failed`、`not-delivered`；不得把 OS-level 注入後的未知結果報成成功。
* 在 Brave native host lookup 與每個目標 OS 的焦點/輸入行為有實機證據之前，不要承諾 keyboard injection、trusted shortcut、兩目標同時送達、或 exactly-once delivery。
