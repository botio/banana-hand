# Banana Hand · App Icon 設計 Spec v2（三套邏輯的唯一共同輸入）

> 使用者已直接指定三個 icon 構圖方向。三個獨立 subagent 只看本 spec、
> `brand-spec.md` 與**自己**被分派的 A/B/C；互不參考、不可把方向換題或混成一版。

## 產品與品牌
**Banana Hand** 是跨平台（Windows / macOS / Linux）桌面 app + Chrome/Firefox extension。
它把使用者選的一個快捷鍵 chord，透過原生輸入注入，盡力發送至兩個 browser tab。
受眾是開發者、鍵盤流與瀏覽器自動化使用者。產品氣質是：**精準專業的工具**，有一點
Banana 的溫暖與玩味；不是兒童 app，也不是抽象 SaaS。

## 本輪使用者指定的三個構圖（不可另創第四題）
- **A｜香蕉一串**：清楚可數的**五根香蕉**，從共用果梗垂落，第一眼就是「一串香蕉」。
- **B｜鍵帽夾香蕉**：**兩個立體鍵帽**，中間明確夾著**一根香蕉**；鍵帽是工具性，香蕉是品牌。
- **C｜香蕉人**：一個**可愛香蕉人**角色，香蕉果身是主體，極簡五官與粗壯小手腳服務角色辨識。

產品的「一個 shortcut → 兩個 tab」仍是次級語意：B 可最直接承載；A/C 可以在
共同果梗、雙手勢、兩側細節或角色姿態中暗示，但**絕不可搶走使用者指定的香蕉主題**。
每版完成時都要寫一句「form 來自內容的哪裡」。

## 🔴 品質標準（前版失敗點：扁平、廉價、沒有質感）
這不是出單色 SVG。每版必須是**高保真、頂級工作室等級的 app icon**，讓人看不出是 AI 做的：
- 主光固定左上；有 fill 與 rim light，香蕉曲面必須有合理亮暗過渡。
- 有空間層次：柔和投影、接觸陰影（ambient occlusion）、輕微挤出/內凹或玻璃/陶瓷/黏土感。
- 香蕉皮有克制的材質：柔和光澤、少量棕色 speckle、深色果梗；細節集中在一處做到 120%。
- 鍵帽（B）要有真實鍵帽體積、bevel、鍵縫陰影；香蕉人（C）要有可信的角色體積與清楚輪廓。
- 絕不能是粗陋 clip-art、普通平塗、廉價 emoji，也不可把每個表面濫用漸層。
- 16px 下仍能辨識該方向：A 看出一串、B 看出兩個鍵帽和香蕉、C 看出香蕉角色。

## 技術輸出（全部統一，方便橫比）
- 產出一張 app icon：**1024 × 1024**。
- 畫布外框透明；中間是近似 macOS squircle 的圓角方塊 tile（radius 約 230px），tile 外的四角
  必須是 **alpha=0**。這張透明 PNG 後續同時供 Tauri、Chrome、Firefox 使用。
- 每個 subagent 只交付一個**純 HTML/CSS 單檔**。可內嵌 SVG 作為精確的 icon 幾何骨架，
  但高保真必須由 CSS/SVG gradient、filter、layer、shadow、light 實現；不使用照片、不用 AI 生圖。
- `html, body { margin: 0; background: transparent; }`；頁面只含這個 1024² icon，不放網頁 UI、標題或文字。
- 存檔：`design-demos/app-icon/<assigned-name>.html`。
- 從 repo root 用既有正式管線渲染：
  `node design-demos/app-icon/_render/render-icon.mjs <assigned-name>.html <assigned-name>.png`
  這支 script 用 Playwright + Chromium，固定 1024²、`omitBackground: true`。
- render 後自檢：1024²、左上 corner alpha=0、縮到 16² 時仍清楚。

## 品牌色（只用此色盤，不臨場發明）
- 香蕉黃 `#FFCF2E`：**主角，必用**
- 深葉綠 `#1E5B3A`
- 深炭綠 `#0F2E1E`
- 奶白 `#FFF4D6`
- 琥珀 `#F59E0B`
- 果梗棕 `#8A5A2B`

## 禁忌
- 紫 / 品紅 / 冷藍紫霓虹；GitHub-dark 冷底 + 通用霓虹 glow。
- emoji、文字詞標、水印、放大鏡、火箭、閃電、終端視窗。
- 幼稚扁平香蕉、廉價 clip-art、扁平單色、過度卡通化的五官。
- 不可再改成無香蕉的抽象 H / command / branch icon。
