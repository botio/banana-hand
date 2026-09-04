# 決定多個輸入標本的派送選取範圍

Parent: ../map.md
Type: grilling
Status: resolved

## Question

使用者可新增多個 Input Specimen，並在右側 Dispatch 區選擇它們時，一個 Coordinated Dispatch 的兩個 Browser Tab 是否仍必須共用同一個選定的 Input Specimen，或可各自選用不同標本？此決定會改變既有「同一 Shortcut Chord 派送至兩個目標」、partial outcome 與 60 秒 App-wide Cooldown 的語意。

## Answer

採用單一派送選一個 Input Specimen。使用者可建立多個 Input Specimen，但每一次 Coordinated Dispatch 在派送前只選定其中一個，兩個不同 Browser Tab 都嘗試接收該標本所代表的同一 Shortcut Chord。要改用另一個標本，必須等待 App-wide Cooldown 結束後再發起下一次派送；同一筆派送中不得為兩個目標選不同標本。

## Comments

- Context pointer: 使用者選定 Calibration Desk，要求可新增多個輸入標本並在派送側選擇不同標本。
