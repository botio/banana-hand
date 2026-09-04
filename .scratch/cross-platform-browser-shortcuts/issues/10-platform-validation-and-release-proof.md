# 決定跨平台驗證與發布證據

Parent: ../map.md
Type: grilling
Status: resolved
Blocked by: none

## Question

在桌面堆疊與封包矩陣確定後，定義可對外宣稱 Windows、macOS Apple Silicon、Linux 支援前必須取得的驗證證據：extension／native host 安裝、連線、兩 Tab 選取、Permission Gate、best-effort 派送狀態、60 秒 App-wide Cooldown、重啟後設定還原與 Target Selection 重選。決定每項的目標平台與 browser 組合。

## Answer

每個 release 必須驗證 12 個平台／工作階段／兩目標 browser 配對：Windows x64、macOS Apple Silicon、Linux x64 X11、Linux x64 Wayland portal，各自覆蓋 Chrome–Chrome、Firefox–Firefox、Chrome–Firefox。Brave 不屬首版支援範圍（其 native host lookup 無官方路徑保證），待官方確認或三平台實機證明完成後再行納入。

每個 cell 必須以實機證明 artifact、extension、host manifest 與 protocol 連線；兩個 live Browser Tab 選取與不同目標規則；已知快捷鍵的盡力發送結果；60 秒全域冷卻、重啟後目標重選及快捷鍵庫還原；以及 host 中斷、目標關閉、權限／portal 拒絕與 prepare 後焦點改變均 fail-closed。驗證不得宣稱 trusted input、原子雙目標或 exactly-once。

每個 release 建立不可覆寫的發布證據 manifest，按 cell 記錄 artifact SHA-256、App／host／extension 版本、OS／browser／session 類型、測試結果、失敗輸出及外部保存的螢幕錄影或截圖連結。

## Comments

- Context pointer: `.scratch/cross-platform-browser-shortcuts/map.md`
