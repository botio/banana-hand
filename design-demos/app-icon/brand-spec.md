# Banana Hand · Brand Spec
> 采集日期：2026-09-04
> 资产来源：用户提供的品牌色 + 产品定位（`src-tauri/tauri.conf.json`：productName "Banana Hand"、identifier "dev.bananahand.dispatch"、v0.1.0）
> 资产完整度：**推断**（这是用户自己的产品，无外部官方 logo 可抓取——logo 正是本次设计对象）

## 🎯 核心资产（一等公民）

### Logo
- **狀態：已定稿** — C「香蕉人」方向，使用者核准原話記於 `direction-approved.md`。
- 官方 master：`design-demos/app-icon/direction-c-person.png`（1024²、透明圓角）。
- Production 圖示：`src-tauri/icons/` 與 `extensions/{chromium,firefox}/icons/` 均由該 master 生成；不可拉伸 / 改主色 / 加描邊。
- 禁用变形：定稿后不能拉伸 / 改主色 / 加描边。

### 产品图 / UI 截图
- **无**（本任务是 app **icon**，是 mark 不是产品照片；不需要产品渲染图或 UI 截图）。

## 🎨 辅助资产

### 色板（品牌锚点，主色 = 香蕉黄）
- **Primary（主角，必用）**：香蕉黄 `#FFCF2E`
- **Ink / mark-on-light**：深叶绿 `#1E5B3A`
- **Background（暗场用）**：深炭绿 `#0F2E1E`
- **Soft / 高光**：奶白 `#FFF4D6`
- **Accent（点睛，小面积）**：琥珀 `#F59E0B`
- **Stem / freckle**：果梗棕 `#8A5A2B`
- **禁用色**：紫 / 品红 / 冷蓝紫霓虹（AI-slop）；冷 GitHub 蓝 `#0D1117`+青紫 glow。

### 字型
- Display：Fraunces / Newsreader（高反差衬线）或 Archivo/Space Grotesk（几何无衬线）——按风格逻辑选，过 typography 配对。
- Body / 标签：Inter 或同族（icon 内默认不放文字）。

### 签名细节（「120% 做到」的地方）
- 香蕉的**果皮质感**（fleck 斑点 + 高光 + 茎部棕色）——这是品牌「120% 细节」。
- 主光方向恒定**左上**，三版一致（品牌光照签名）。
- 「一→二」的**分岔**用负空间或双元素承载，不做第三层装饰。

### 禁区
- 幼稚 clip-art 香蕉、emoji 香蕉、紫渐变、冷霓虹、icon 内文字、水印、扁平单色。

### 气质关键词
- **professional tool** × **playful banana** · **precise** · **warm** · **ownable** · **one→two duality**
