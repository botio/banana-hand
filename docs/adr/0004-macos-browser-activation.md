# macOS 桌面 App 在發送前把目標瀏覽器 App 帶到前景

v0.1.8 引入 macOS `InputAdapter::activate`，在 browser adapter 啟用／聚焦與前景閘門之前，先尋找正在執行的 browser，再以 `open -a` 請求啟用。這不代表 App 擁有全部輸入權限，也不證明視窗已成為前景。v0.1.10 的程序列舉改為 `ps -ww -axo comm=`，比對完整執行檔路徑；Firefox bundle 名稱為 `Firefox`，執行檔名稱為 `firefox`，兩者不可假定相同。

## Consequences

前景閘門維持 verify-only：無法驗證即拒絕輸入。Windows／Linux 的 `activate` 目前不執行額外 OS 啟用操作，不由此推論 WebExtension 一定能搶到焦點。同時執行多個 Chrome channel 的辨識限制仍存在，候選順序以 stable 優先，不宣稱已驗證精確 OS 視窗身分。

## 診斷更正（v0.1.10）

先前把「G」認定為終端機／聊天 App，並宣稱 WebExtension 無法啟用背景 browser，均沒有實機證據。使用者確認 v0.1.9 畫面已跳到目標分頁；本次重現發現 `CFStringGetCString` 回傳 Boolean `1` 被誤用為長度，`Google Chrome` 因而被截成 `G`。這是字串轉換錯誤，不能以增加啟用次數修復。
