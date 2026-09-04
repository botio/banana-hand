# 原型化雙分頁選取體驗

Parent: ../map.md
Type: prototype
Status: resolved
Blocked by: 02

## Question

根據已選的 Browser Tab 資料與發送架構，將已選的 Calibration Desk 原型改為左側可新增多個快捷鍵的快捷鍵庫，以及右側單一發送區。驗證兩個下拉選單如何呈現 browser、window 與 tab 識別資訊、同一目標的限制、重新整理與失效狀態、每次發送選取一個快捷鍵、發送按鈕及每個目標的結果。

## Answer

使用者確認 Calibration Desk 改版方向。左側是可新增多個快捷鍵的快捷鍵庫；右側是一次單一發送，從庫中選一個快捷鍵，兩個不同 Browser Tab 必須共用它。目標選取不持久化；新增的原型快捷鍵也僅存在於記憶體。發送後，執行按鈕會進入 60 秒全域冷卻，並只呈現盡力嘗試的結果。已於 1440×900 瀏覽器驗證：快捷鍵新增、選取、發送、冷卻與 partial outcome 都可觀察。

## Comments

- Context pointer: `.scratch/cross-platform-browser-shortcuts/map.md`
- Direction boards: [Calibration Desk](../prototypes/design-demos/calibration-desk.html), [Dispatch Console](../prototypes/design-demos/dispatch-console.html), [Protocol Poster](../prototypes/design-demos/protocol-poster.html).
- Shared design rationale: [design spec](../prototypes/design-spec.md).
- Selected direction: `Calibration Desk`; revised asset keeps one shared 快捷鍵 per 發送.
