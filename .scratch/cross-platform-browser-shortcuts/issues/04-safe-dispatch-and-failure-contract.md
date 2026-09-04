# 定義安全派送與失敗契約

Parent: ../map.md
Type: grilling
Status: resolved
Blocked by: 02

## Question

定義 Coordinated Dispatch 的可觀察行為：兩個 Browser Tab 的前景切換順序、焦點恢復的 best-effort 邊界、Tab 已關閉／重載／同名時的驗證、單一目標派送失敗時是否繼續另一目標、結果呈現，以及任何需要使用者明確確認的高風險輸入。

## Answer

- 派送流程若在任一次注入前發現前景被使用者、OS 或其他程式改變，或 adapter 無法確認指定 browser window 仍可接收輸入，立即停止；App 不得再次搶回焦點或繼續注入。
- 兩個 Target Selection 必須是不同的 Browser Tab；UI 禁止第二個下拉選單選取與第一個相同的目標。
- 執行完畢後，只有在前景仍可確認是 App 最後啟用的目標時，才 best-effort 恢復派送前的前景視窗。使用者中途切換焦點時，App 不得恢復或奪回焦點。
- `執行`按鈕是唯一的使用者確認。按下前 UI 顯示兩個目標及 Shortcut Chord；不使用每次派送的確認 modal。
- App 逐一顯示 `rejected-stale`、`permission-blocked`、`focus-failed`、`input-attempted`、`partial` 結果。`input-attempted` 只表示 adapter 已嘗試，絕不表示 browser、網頁或快捷鍵確實收到。

## Comments

- Context pointer: `.scratch/cross-platform-browser-shortcuts/map.md`
