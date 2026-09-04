# 決定重複派送冷卻策略

Parent: ../map.md
Type: grilling
Status: resolved

## Question

為避免重新發送，Coordinated Dispatch 必須在一次接受派送後的 60 秒內拒絕再次執行。冷卻應如何界定「重複」與顯示給使用者？

## Answer

採用 App-wide Cooldown：任何一次使用者啟動且被 App 接受的 Coordinated Dispatch，都啟動全 App 60 秒不可再次派送的計時器；不依 Browser Tab 或 Shortcut Chord 區分。計時器期間，執行按鈕必須停用並顯示剩餘秒數。即使第一次派送僅部分成功或失敗，冷卻仍生效，避免使用者重複送出可能已抵達的指令。

## Comments

- Claimed for live decision with the user.
