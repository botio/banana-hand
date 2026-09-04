# 決定設定持久化與分頁重選規則

Parent: ../map.md
Type: grilling
Status: resolved
Blocked by: none

## Question

定義哪些使用者設定在 App 重啟後自動還原，以及哪些狀態只能存在當次執行期。既定需求是 Browser Tab 不得持久化：每次開啟 App 都必須重新選擇兩個目標。確認首版要持久化的使用者偏好、儲存位置與版本升級策略，並確認 App-wide Cooldown 是否只能是執行期狀態。

## Answer

持久化設定只包含快捷鍵庫：每筆有穩定 ID、名稱、單一組合按鍵與排序，存於 Tauri Store。不得持久化 Browser Tab、目標選取、發送紀錄、發送結果、extension 連線、session nonce、host capability token 或權限狀態；每次 App 啟動都重新建立這些 session 狀態並要求重選目標。

60 秒全域冷卻僅限 App 執行期；正常結束或崩潰後即失效。Tauri Store 使用整數 `schemaVersion` 與僅向前的 Rust migration；migration 前建立同目錄單份備份。遇到損壞、未知更高版本或 migration 失敗時，保留原檔、以空快捷鍵庫啟動並顯示可復原錯誤。

## Comments

- Context pointer: 使用者要求記錄設定，但 Browser Tab 每次開啟必須重新選擇。
