# Banana Hand · Direction Approved
> Phase 5 gate file · 記錄三方向真實 render 與使用者有效選擇
> 日期：2026-09-04
> 共同設計 spec：`design-demos/app-icon/ICON-SPEC.md`
> 品牌資產 spec：`design-demos/app-icon/brand-spec.md`

## 展示的三個高保真方向

### A · 五根香蕉成一串
- Huashu logic：秒數輪盤 `#13` — Glassmorphism Bento（以深綠玻璃 + 香蕉黃暖光轉譯，未使用冷紫霓虹）。
- HTML：`design-demos/app-icon/direction-a-bunch.html`
- Master render：`design-demos/app-icon/direction-a-bunch.png`（1024×1024、透明四角）
- 16px proof：`design-demos/app-icon/direction-a-bunch-16.png`
- Form：共同冠部串聯精確五根成熟香蕉；雙主叉僅暗示 one→two。

### B · 兩個鍵帽夾一根香蕉
- Huashu logic：現實參照／標杆遷移 — Apple Magic Keyboard 的低剖面、鍵縫、倒角、側壁和接觸陰影（僅取設計語言，未使用官方資產）。
- Benchmark source：https://support.apple.com/en-us/112443
- HTML：`design-demos/app-icon/direction-b-keycaps.html`
- Master render：`design-demos/app-icon/direction-b-keycaps.png`（1024×1024、透明四角）
- 16px proof：`design-demos/app-icon/direction-b-keycaps-16.png`
- Form：使用者指定的 keycap + banana + keycap，將一個 shortcut 被兩個目標承接轉為可信的物理接觸。

### C · 可愛香蕉人
- Huashu logic：最佳設計師／頂級定制 — Collins 等級的「單一可擁有中心概念」與可延展角色系統。
- HTML：`design-demos/app-icon/direction-c-person.html`
- **Approved master render**：`design-demos/app-icon/direction-c-person.png`（1024×1024、透明四角）
- 16px proof：`design-demos/app-icon/direction-c-person-16.png`
- Form：使用者指定的香蕉人；向左右展開的雙手表達一個 shortcut 分送至兩個 tab。

## 使用者選擇（原話）
> 「用 香蕉人 C 吧」

## 核准後執行範圍
以 `direction-c-person.png` 為唯一 master：
1. 用 `tauri icon` 生成 desktop app 的 `.png` / `.ico` / `.icns` 全套圖示。
2. 生成 Chrome 與 Firefox extension 的 16 / 32 / 48 / 128px 圖示，並將兩份 manifest 的 `icons` 指向它們。
3. 重打包 Firefox `.xpi`，驗證 archive 與圖示引用。
