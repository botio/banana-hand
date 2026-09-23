# 15. Windows 同視窗雙分頁 Ctrl+R 漏送：激活順序回歸

Type: task
Status: resolved
Supersedes: 14 (前景驗證的 Windows 側盲點)

## 更正與 Windows 重現證據（2026-09-23）

下方早期「Root cause」及 2026-09-20 的 renderer IPC／延遲不足解釋
均屬未驗證推測，不可作為已確認根因。`已嘗試發送` 不證明送達，
也不證明目標02 收到了目標01 的指令。原 11 個 extension 測試
與單輪 F8 smoke 都未涵蓋使用者的 Ctrl+R 重新整理情境。

使用者補充：Chrome 同一視窗、不同 TAB、原生 Ctrl+R。
Windows runner 35822032889 用 HTTP 文件請求計數實際重現：
第一輪 [0,2]（目標01 未重載，目標02 重載兩次），
等待真正60秒冷卻後第二輪 [1,1]；兩輪 UI 均顯示已嘗試發送。

只調整 Chromium `prepareTarget` 的激活順序：
先 `windows.update({focused:true})`，再 `tabs.update({active:true})`。
保留既有等待值、SendInput、確認與冷卻，不新增重送。
Windows runner 35822348399 同一測試兩輪皆 [1,1]。
這支持「視窗聚焦與分頁激活順序影響 Ctrl+R 目標」，
不代表已證明 Chromium 內部 IPC 機制或所有平台／網站都正常。
Firefox 此次未修改，因尚無對應重現。

回歸腳本 `scripts/verify-windows-browser.mjs` 預設測原生 Ctrl+R，
以兩個分頁各重載一次為通過條件；不攔截預設按鍵、不透過 API 重載。
設定 `BANANA_SMOKE_CHORD=F8` 可保留原收鍵檢查。診斷分支尚未發布。

只交換 API 順序不足以穩定修復：runner 35822660723 仍是第一輪 [0,2]、第二輪 [1,1]。
後續改為先要求視窗聚焦，輪詢確認 `window.focused` 後才 `tabs.update`。
獨立冷啟動 35823048127 與 35825372449 兩輪皆 [1,1]。
這仍是同一視窗、原生 Ctrl+R、測試頁的證據，不是所有網站的保證。

## Background

Windows 使用者回報：一輪發送裡 **目標01 沒有觸發、但 目標02 有觸發**，
而且**尤其發生在第二輪（之後）發送**。第一輪通常兩邊都成功。

## Root cause

dispatch 只有在 `prepared {ready:true}` **之後**才注入快捷鍵（桌面端
`prepare_target` 等 `prepared`、再 `verify_windows_foreground`、再 `send`）。
但 extension 的 `prepareTarget` 在 `tabs.update({active:true})` +
`windows.update({focused:true})` 的 **promise 一 resolve 就回報 ready**——
兩個 API 只代表「要求被接受」，不代表目標 tab 已實際取得輸入焦點
（與 issue 14 記載的 macOS 非同步視窗激活同一類競態）。

Windows 的 `verify_windows_foreground` 只比對**前景 process 名稱**
（`chrome.exe`/`firefox.exe`），不知道哪個 tab/視窗在前景。因此：

- 目標都在同一瀏覽器時，切換 tab/視窗**不會改變前景 process**，
  verify 立刻通過（僅 +100ms），等於完全沒有屏障。
- 第二輪開始時，上一輪被服務的目標（例如目標02 的 tab/視窗）已是
  前景並保持 active。`prepare(01)` 請求切到目標01，切換仍在 **非同步
  進行中**時桌面端就 `SendInput` → 快捷鍵送進**仍是 active 的目標02**
  →「目標01 沒觸發、目標02 有觸發」。接著 `prepare(02)` 激活目標02
  （本就 active，無需實質切換）→ 目標02 收到自己的鍵 →「目標02 有觸發」。
- 第一輪通常成功，因為瀏覽器從背景被激活時 OS 前景切換本身提供了
  足夠時間讓 tab 提交；第二輪才暴露（上一輪留下的 active 目標仍在原地）。

## Decision

讓 `prepared` handshake **說真話**：extension 在請求激活後，**有界輪詢
重新讀取**目標 tab 與 window，確認目標確實是「active tab 且視窗 focused」
才回報 `ready:true`；無法在期限內確認就回報 `focus_failed`（fail-closed，
桌面端顯示失敗而非把鍵錯送到別處）。確認後再給 renderer 一小段
`FOCUS_SETTLE_MS` 讓新激活的 tab 提交鍵盤焦點。

- chromium 與 firefox extension 同步修正（同一 bug class）。
- 常數：`FOCUS_CONFIRM_TRIES=20`、`FOCUS_CONFIRM_INTERVAL_MS=80`
  （最壞 ~1.6s）、`FOCUS_SETTLE_MS=100`——遠低於桌面端
  `PREPARE_TIMEOUT=3s`，不會觸發桌面超時。
- 桌面端 `verify_windows_foreground` 維持 process 級粗閘；真正的屏障
  移到 truthy 的 handshake。
- 不新增搶焦點重試（不 claim delivery）；未確認即 fail-closed。

## Validation

- `npm run test:extensions` 11/11 綠，新增四項 per-browser 測試：
  - `prepare withholds ready:true until the target tab is active in a focused window`：
    聚焦未落地時不得先報 ready；焦點成功提交後才報 `ready:true`。
  - `prepare reports focus_failed and never ready:true when the target never gains focus`：
    永不聚焦 → `focus_failed`，絕不偽造 ready。
  - `stales target reports rejected_stale before any activation`：
    session 驗證仍最先、激活前拒絕。
  - 既有 rejected handshake／alarm reconnect 測試不受影響。
- `node --check` 兩支 extension 通過；`extension-packaging` 1/1 通過。
- 本機為 Linux，無法跑 Windows 實機輸入；此為 code-level 根因修正，
  Windows 實機送達仍待使用者確認（與 issue 14 一致的驗證邊界）。

## Comments

### 2026-09-20：第二輪仍失敗——settle 不足

使用者重載 extension 後回報「第二輪、目標01」仍顯示**已嘗試發送**，但目標01 頁面沒
反應、目標02 正常。這代表桌面端走完全部閘門（`prepared ready:true`、前景驗證、
`SendInput` 都成功），鍵卻沒進目標01——extension 的 `window.focused`/`tab.active`
flag 為真時，**renderer 的實際鍵盤焦點尚未經 IPC 提交**，第一次注入被前一輪仍活著的
目標02 吃掉。

把 `FOCUS_SETTLE_MS` 由 100ms 拉到 **400ms**（送出前給 renderer 足夠時間把焦點真正
交到目標 tab；confirm 迴圈仍設 1.6s 上限，總和最壞 2.0s，低於桌面 `PREPARE_TIMEOUT`
3s）。確認這不是 spec 推翻：仍是「確認 + settle + fail-closed」的同一模式，只是把
盲猜的定時窗口改得保守、安全。