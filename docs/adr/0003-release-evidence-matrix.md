# 以完整矩陣作為支援宣稱門檻

每個 release 只有在 Windows x64、macOS Apple Silicon、Linux x64 X11 與 Linux x64 Wayland portal 的三種兩目標 browser 配對（Chrome–Chrome、Firefox–Firefox、Chrome–Firefox）都具有實機發布證據時，才能宣稱支援 Chrome 與 Firefox。每個 cell 的 artifact、host／extension 連線、選取、發送契約、權限與失敗路徑都必須驗證。Brave 不屬於首版支援範圍：其 native host lookup 缺乏官方路徑保證，待官方確認或三平台實機證明完成後再行納入。

## Consequences

release 建立按 cell 分組的不可覆寫 manifest，保存 artifact SHA-256、各元件版本、環境、結果、失敗輸出與外部影音證據。測試可證明盡力發送與 fail-closed 行為，但不宣稱 trusted input、原子雙目標或 exactly-once。