# 雙分頁快捷鍵派送 App — UI 方向規格

## 設計目的

這不是瀏覽器管理器，也不是巨集工具。它是一個使用者在已理解風險後，對兩個已選 Browser Tab 做一次有節制、可觀察、不可重送的 Coordinated Dispatch 的桌面控制面。核心使用情境是：使用者打開桌面 App，確認兩個 Connected Browser 已回報本次 session 的 Tab，選定不同的兩個目標，設定一個單一 Shortcut Chord，並按下 `執行`。介面必須讓使用者在按下前清楚看見「會對哪兩個 Tab 嘗試哪個按鍵」，而不是把不可靠的 OS 輸入偽裝成確定成功。第一受眾是同時操作多個 browser 的技術使用者；他們需要迅速、精準、低干擾的操作面，而不是儀表板式裝飾。

## 必須呈現的內容與狀態

每個方向都必須保留相同的真實內容：兩個明確標為第一、第二的 Target Selection；每個選項至少帶 browser、window 與 tab title 的可區分資訊；兩個選項不可相同；可編輯 Shortcut Chord；重新整理 session；顯示 App-wide Cooldown；顯示 `rejected-stale`、`permission-blocked`、`focus-failed`、`input-attempted`、`partial` 的語意；以及「只嘗試派送、不保證送達」的誠實邊界。Tab metadata 只在當次畫面出現，不暗示會持久化；重新開啟 App 必須重選 Tab。不要使用假統計、使用者頭像、裝飾性圖示、漸變科技背景、emoji 或虛構品牌資產。

## 視覺與互動原則

畫布設定為桌面 1440×900 的單一工具視窗。資訊的視線順序是：目標識別 → Shortcut Chord → 執行確認 → 結果／限制。行動按鈕是唯一高對比重點，但不可採用危險的「成功綠」語意，因為按下只表示開始一次 best-effort 嘗試。用文字、細線、數字與可見的狀態節點建立可信度；只有真正承載狀態的 UI 才能使用圖示或色點。色彩從產品語境採樣：瀏覽器目標是短暫 session 資料，因此採低飽和的石墨、中性紙白或深炭，搭配單一暖 amber 代表「正在受控的輸入嘗試」；它不假借任何瀏覽器品牌色作為自身品牌。字體可使用開源的 Newsreader、DM Mono、Space Grotesk、IBM Plex Sans 或系統 fallback；顯示字體避免 Inter/Roboto 作為主角。

## 三個方向要回答的問題

方向 A「Calibration Desk」要驗證：在溫暖、出版級的工作台中，兩個目標並列是否最容易比較並確認。方向 B「Dispatch Console」要驗證：把權限、前景競態、冷卻與結果固定在操作 rail，是否更適合風險敏感使用者。方向 C「Protocol Poster」要驗證：以極致瑞士式步驟與巨型數字展示一次受控交易，是否最能避免使用者誤認為是一般巨集工具。三個方向必須有互異的布局骨架、資訊階層與主要操作可見性；所有方向都是 throwaway prototype，沒有真實 browser 連線或持久化。

## 圖片與品牌判斷

此為純工具／控制面，沒有內容必需的照片、產品圖或具名品牌識別；圖片不是資訊的一部分，因此刻意不用圖片。真實資訊由 Browser Tab 的結構化 metadata、狀態與操作順序承載。視覺母題分別是「校準台上的兩枚標本」、「飛航前檢清單」、「一次不可重播的操作協定」。
