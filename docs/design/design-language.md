# TomatoDesk 设计语言

- 状态：生效中（V0.3 阶段三落地）
- 来源：博客 `AbelTomato.github.io` 的 `src/styles/global.css`、`components/ui/{card,button,input}.tsx`、`components/layout/Header.astro`、`components/blog/BlogPostCard.astro`
- 代码基座：`src/styles/index.css`（token）、`src/components/ui/`（组件）
- 可执行检查：`pnpm check:design`；开发模式预览：`?styleguide`

本文件是视觉判断的唯一依据。改样式前先看这里，改完用检查脚本兜底。

## 一、原则

1. **中性优先**。界面由灰阶构成，强调靠字重、间距、边框与位置，不靠颜色。
2. **颜色是信息，不是装饰**。语义色只出现在状态条、提示条竖线、错误文字这三处。
3. **等宽承载元信息**。时间、计数、编号、标签一律 `font-mono text-xs`。
4. **分隔线成组**。列表用 `divide-y` 分组，不用卡片边框堆叠。
5. **模糊只给浮层**。`backdrop-blur` 仅限吸顶栏、弹窗、浮层，列表与面板保持不透明。
6. **文案不硬翻**。GitHub 原生概念沿用英文原文，自有概念与完整句子用中文，详见第七节。

## 二、色板

暗色为默认（`:root`），浅色由 `@media (prefers-color-scheme: light)` 覆盖，不提供手动开关。

| Token                  | 暗色                        | 浅色                        | 用途                |
| ---------------------- | --------------------------- | --------------------------- | ------------------- |
| `--background`         | `oklch(0.145 0 0)`          | `oklch(1 0 0)`              | 窗口底色            |
| `--foreground`         | `oklch(0.985 0 0)`          | `oklch(0.145 0 0)`          | 正文                |
| `--card`               | `oklch(0.205 0 0)`          | `oklch(1 0 0)`              | 面板、详情区        |
| `--muted` / `--accent` | `oklch(0.269 0 0)`          | `oklch(0.97 0 0)`           | 次级面、hover 底    |
| `--muted-foreground`   | `oklch(0.708 0 0)`          | `oklch(0.556 0 0)`          | 元信息、说明文字    |
| `--primary`            | `oklch(0.922 0 0)`          | `oklch(0.205 0 0)`          | 主按钮（近白/近黑） |
| `--border`             | `oklch(1 0 0 / 10%)`        | `oklch(0.922 0 0)`          | 分隔线、描边        |
| `--input`              | `oklch(1 0 0 / 15%)`        | `oklch(0.922 0 0)`          | 输入框描边          |
| `--link`               | `#60a5fa`                   | `#2563eb`                   | 链接、激活态        |
| `--success`            | `oklch(0.72 0.15 155)`      | `oklch(0.52 0.13 155)`      | 成功提示            |
| `--warning`            | `oklch(0.79 0.14 82)`       | `oklch(0.56 0.12 75)`       | 待配置、迁移提示    |
| `--destructive`        | `oklch(0.704 0.191 22.216)` | `oklch(0.577 0.245 27.325)` | 错误、删除          |

### 状态色例外

GitHub 线程状态需要可辨识，但不允许破坏中性基调，因此单独开三个 token：

| Token            | 暗色                   | 浅色                   | 语义   |
| ---------------- | ---------------------- | ---------------------- | ------ |
| `--state-open`   | `oklch(0.72 0.15 155)` | `oklch(0.55 0.13 155)` | 打开中 |
| `--state-merged` | `oklch(0.68 0.16 300)` | `oklch(0.5 0.18 300)`  | 已合并 |
| `--state-closed` | `oklch(0.68 0.17 25)`  | `oklch(0.55 0.2 27)`   | 已关闭 |

规则：

- 只允许出现在 `<StateMark>` 的 2px 竖条与小号文字上，或详情页顶部的同一位置。
- `draft` 走中性色（`muted-foreground`），不单独开色。
- 不做彩色填充徽标、不做彩色边框、不给整行上色。
- 其余场景（条目状态、同步结果、标签）一律中性。

### 对比度实测（WCAG AA，正文要求 ≥ 4.5:1）

| 前景               | 背景                | 暗色    | 浅色    |
| ------------------ | ------------------- | ------- | ------- |
| `muted-foreground` | `background`        | 7.63:1  | 4.73:1  |
| `muted-foreground` | `card`              | 6.91:1  | 4.73:1  |
| `foreground`       | `background`        | 18.96:1 | 19.79:1 |
| `link`             | `background`/`card` | 7.79:1  | 5.12:1  |
| `state-open`       | `card`              | 7.69:1  | 4.56:1  |
| `state-merged`     | `card`              | 5.85:1  | 6.55:1  |
| `state-closed`     | `card`              | 5.76:1  | 5.37:1  |

浅色 `--link` 从 `#3b82f6` 调到 `#2563eb`：前者在纯白上只有 3.68:1，正文级链接不达标；后者 5.12:1，同时与博客浅色态的蓝色 hover 一致。

## 三、字体与排版

```css
font-family: "JetBrains Mono Variable", "LXGW WenKai", ui-monospace, monospace;
```

- 拉丁字符与数字走 JetBrains Mono，中文回落霞鹜文楷；全站统一，不区分标题与正文。
- 霞鹜文楷只打包 300 字重的 woff2（约 8.8 MB 字体总量），按 400 声明，粗体由浏览器合成。
- 字号只有五档：

| 用途     | 类名                                      |
| -------- | ----------------------------------------- |
| 元信息   | `font-mono text-xs text-muted-foreground` |
| 正文     | `text-sm leading-6`（长文 `leading-7`）   |
| 强调     | `text-base font-medium`                   |
| 页面标题 | `text-lg font-bold tracking-tight`        |
| 详情标题 | `text-xl font-bold tracking-tight`        |

- 不再使用 `text-[10px]` / `text-[11px]` 这类一次性字号，元信息统一 `text-xs`。
- 章节标题用 `text-sm font-bold` + `border-b border-border pb-2`（对应博客文章 h2 的下划线）。

## 四、尺寸与栅格

- 圆角：`--radius: 0.625rem`（10px），派生 `sm 6 / md 8 / lg 10 / xl 14 / 2xl 18`。
- 控件高度：按钮与输入 `h-8`，小号 `h-7`，图标按钮 `size-8`。
- 内边距：列表行 `px-4 py-3`（宽列表 `px-5 py-3 sm:px-7`），面板 `p-4`，页级 `p-5 sm:p-7`。
- 页面左右留白：`px-5 sm:px-7`；侧栏 `w-60`、`px-4 py-4 lg:px-5 lg:py-6`。
- 双栏：`lg:grid-cols-[minmax(18rem,0.9fr)_minmax(22rem,1.1fr)]`，列表在左、详情在右，分界线用 `border-r`。
- 阅读列：详情区正文 `max-w-2xl` 居中，避免长行。

## 五、组件配方

组件都在 `src/components/ui/`，页面只组合、不拼类名。

| 组件            | 用法与样式                                                                                                                                                                                          |
| --------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Button`        | 变体 `default`（近白/近黑底）/ `outline`（`border-input bg-input/20`）/ `ghost` / `destructive`（红字，hover 才上底色）。尺寸 `sm h-7` / `md h-8` / `lg h-9` / `icon size-8`。按下 `translate-y-px` |
| `Input`         | `h-8 rounded-lg border border-input bg-transparent px-2.5 text-sm`，聚焦 `ring-3 ring-ring/50`                                                                                                      |
| `Textarea`      | 同上，`min-h-20 resize-y py-1.5 leading-6`；需要更高用 `rows`，不要叠 `min-h-*`                                                                                                                     |
| `Panel`         | `rounded-xl border border-border bg-card`；`blur` 打开时才是 `bg-card/72 ring-1 backdrop-blur-xl`                                                                                                   |
| `Badge`         | 小号等宽大写：`rounded bg-muted px-1.5 py-0.5 font-mono text-xs uppercase tracking-wider`；`success/warning/destructive` 只换文字色，不加底色                                                       |
| `Row`           | 导航行：`h-9 rounded-md px-3`，hover `bg-accent/60`，激活蓝字 + 左侧 2px 竖条                                                                                                                       |
| `ListItem`      | 列表行：`flex-col gap-1.5`，`flush` 去掉圆角配合 `divide-y`，激活 `bg-accent/60` + 左侧 2px 竖条                                                                                                    |
| `ListItemTitle` | `text-sm font-medium`，`clamp` 控制 1/2 行                                                                                                                                                          |
| `ListItemMeta`  | `flex flex-wrap gap-x-3 font-mono text-xs text-muted-foreground`                                                                                                                                    |
| `StateMark`     | 2px 竖条 + 小号等宽文字，`tone` 取 `open/merged/closed/neutral`                                                                                                                                     |
| `Tabs`          | `border-b` + 激活项 2px 蓝色下划线，计数用 `font-mono text-xs`；`trailing` 放右侧补充信息                                                                                                           |
| `.callout`      | 提示条：中性底 + 左侧 2px 强调条，配 `callout-info/success/warning/error` 改竖线颜色                                                                                                                |
| `Markdown`      | GitHub 风格正文：组件在 `src/components/markdown/`，排版走 `.markdown`；只用于 PR / Issue 正文与评论，标题与摘要不渲染                                                                              |

### 为什么提示条用 CSS 类而不是工具类

`cn()` 只做字符串拼接，不处理 Tailwind 冲突。`border-border`（`border-color` 简写）与 `border-l-destructive`（`border-left-color`）同时出现时，谁生效取决于样式表顺序，不可预期。因此：

- 提示条统一用 `.callout*` 组件类，颜色写在 CSS 里。
- 组件 base 不写会被变体覆盖的属性（例如 `Button` 的 `border-*` 颜色交给变体）。
- 调用处不要用 `className` 覆盖组件已有的同属性工具类（如给 `ListItem` 再传 `rounded-none`，改用 `flush`）。

### Markdown 正文

PR / Issue 详情页的正文与评论走 `src/components/markdown/Markdown.tsx`，排版层是 `index.css` 的 `.markdown`。**不引入 `@tailwindcss/typography`**：`prose` 的默认字号阶梯与颜色和本项目的五档字号、中性色板冲突，覆盖成本高于自建。

| 元素      | 配方                                                      |
| --------- | --------------------------------------------------------- |
| 正文      | `text-sm`，行高 1.75rem                                   |
| h1        | 1.125rem 粗体                                             |
| h2        | 1rem 粗体 + `border-b`                                    |
| h3        | 0.875rem 粗体                                             |
| h4–h6     | 0.875rem 中等字重                                         |
| 行内代码  | `bg-muted` + 等宽 0.75rem                                 |
| 代码块    | `bg-background` + `border-border`，等宽 0.75rem，横向滚动 |
| 引用      | 左侧 2px `border` + `muted-foreground`                    |
| 表格      | 单元格底边线，外层横向滚动                                |
| 链接      | `--link`，hover 下划线                                    |
| 图片      | 最大宽度 100%，圆角 + 描边                                |
| 任务列表  | 只读复选框，`accent-color: --link`                        |
| `details` | 描边圆角容器，`summary` 用 `muted-foreground`             |

规则：

- 原始 HTML 经 `rehype-raw` 解析后**必须**过 `rehype-sanitize` 白名单；链接只放行 http / https / mailto，图片只放行 https。
- 外链与锚点一律交给 `openExternal`，不在 WebView 内导航。
- 标题与摘要不渲染 Markdown：标题要进列表行、面包屑与复制场景，摘要依赖 `line-clamp`。
- 改 Markdown 样式只改 `.markdown`，不要在页面里给渲染结果补类名。

## 六、页面结构

四个界面共用同一套骨架：

```
┌ 侧栏 w-60 ─┐┌ 内容 ────────────────────────────────┐
│ 品牌 + 版本 ││ 页头：标题 text-lg + 计数 mono xs     │
│ 快速收集    ││       说明 text-sm muted             │
│ 导航 Row    ││       （可选）搜索框 h-8 + 操作按钮   │
│ 快捷键提示  │├ Tabs（border-b，激活蓝色下划线）─────┤
└────────────┘│ 列表（divide-y + ListItem）│ 概览 bg-card │
```

- 页头不再使用英文眉标（Inbox / GitHub / Search / Settings），标题直接用中文。
- 计数、同步时间等信息放在标题右侧或列表行内，用 mono 小号。
- 空态：一行说明 + 一行补充，居中，不加边框卡片。

### 滚动由内容区自己负责

外壳在 `lg` 以上锁死视口高度（`h-screen` + `overflow-hidden`），页面本身不滚动，滚动条只出现在内容区的滚动容器里。这样列表与右侧面板才是两条独立的滚动条，不会互相带动。

因此新增页面或新增一栏时，必须自带滚动容器：

```
根：flex min-h-0 flex-1 flex-col
└ 滚动区：min-h-0 flex-1 overflow-y-auto   ← 少了 min-h-0 就不滚
```

`lg` 以下（窗口窄于 1024px）外壳退回 `min-h-screen` 文档流，整页滚动、两栏上下堆叠——那时没有「两栏」可言。

### 列表 → 概览 → 详情页

三列信息密度是逐级放大的，别把后一级的内容塞进前一级：

| 层级     | 位置                         | 放什么                                                                                         |
| -------- | ---------------------------- | ---------------------------------------------------------------------------------------------- |
| 列表行   | 左栏 `ListItem`              | 标题、`StateMark`、编号、关系、diff 摘要、评论数、时间                                         |
| 概览面板 | 右栏 `bg-card`               | 标题、作者、状态与关系、操作按钮、摘要（`summary`，缺省从正文压平截 200 字）、`查看完整内容 →` |
| 详情页   | 整块内容区覆盖，带面包屑返回 | 完整正文、全部时间点、变更规模、元信息表                                                       |

概览面板只铺摘要，不铺正文——正文一长，右栏就变成第二份列表，还得再滚一次。

详情页以覆盖层形式挂在仓库详情内部（`absolute inset-0 z-10`），底层加 `inert`：列表的选中项、筛选与滚动位置原地保留，返回即是原样，不用重新加载。**不要**把它做成独立路由——那样每次返回都会重建列表、丢掉滚动位置。

## 七、文案

一条原则：**GitHub 原生概念沿用英文，自有概念与完整句子用中文。** 硬翻会多一层翻译成本，读起来也更怪。

| 位置       | 写法                                                                 | 说明                                                        |
| ---------- | -------------------------------------------------------------------- | ----------------------------------------------------------- |
| 线程状态   | `open` `draft` `merged` `closed`                                     | 与 GitHub 网页端一致；不写「打开 / 草稿 / 已合并 / 已关闭」 |
| 与我的关系 | `author` `review requested` `reviewed` `assigned`                    | 同上，与 GitHub PR 列表的标签同词                           |
| 状态筛选   | `open` `all`                                                         | 「打开中 / 全部」别扭；`open` 含草稿，与后端筛选语义对齐    |
| 评审范围   | `review requested` `reviewed`                                        | 与线程行的关系标签同词，切换时不会觉得是两个概念            |
| Release    | `pre-release` `draft`                                                | `Badge` 渲染为等宽大写                                      |
| 仓库标记   | `archived` `private`                                                 | 同上                                                        |
| 通用名词   | `PR` `Issue` `Pull Requests` `Release` `Stars` `Forks` `Open issues` | 已在开发者日常语汇里，翻译反而多余                          |
| 自有概念   | 概览、评审、我的条目、条目、待处理、已归档                           | 这些不是 GitHub 的东西，用英文要额外解释                    |
| 完整句子   | 一律中文                                                             | 提示、空态、错误、tooltip 都是句子，不是标签                |

- 同一屏里英文标签与中文句子并存是刻意的：标签是扫视用的短词，句子是用来读的。
- 计数行写成 `PR 6`、`Issue 24`、`评审 3`，标签与数字之间留一个空格，不加冒号。
- 新增一处文案时先问：这个词是 GitHub 的，还是我们自己的？GitHub 的就别翻。

## 八、禁用项

`pnpm check:design` 会扫描 `src/`，出现以下写法即失败：

| 禁用                                                                 | 原因                                         |
| -------------------------------------------------------------------- | -------------------------------------------- |
| `tomato-*`、`ink-*`                                                  | 旧色板 token                                 |
| `bg-canvas`、`bg-surface`、`border-line`                             | 旧语义类名                                   |
| `text-white`                                                         | 硬编码白色，暗浅主题不通                     |
| `tracking-[0.18em]`                                                  | 旧眉标字距                                   |
| `bg-{red,green,blue,purple,amber,yellow,emerald,slate}-{50,100,200}` | 彩色填充底                                   |
| `backdrop-blur`（`Panel.tsx` 之外）                                  | 大面积模糊在 WebView2 掉帧                   |
| `dangerouslySetInnerHTML`                                            | Markdown 链路不产生 innerHTML，ESLint 已禁止 |

## 九、与博客的组件对照

| 场景     | 博客                                                                             | TomatoDesk                                                       |
| -------- | -------------------------------------------------------------------------------- | ---------------------------------------------------------------- |
| 导航项   | `rounded-md px-3 py-2 text-sm hover:bg-accent`，激活加粗下划线                   | `Row`：同尺寸，激活改为蓝字 + 左侧 2px 竖条（侧栏更省横向空间）  |
| 文章卡片 | `rounded-xl border bg-white/70 shadow-lg backdrop-blur-xl`                       | 列表改为 `divide-y` + `ListItem`，不模糊、不投影（滚动性能）     |
| 标签     | `rounded bg-gray-100 px-2 py-0.5 text-[10px] font-bold uppercase tracking-wider` | `Badge`：`text-xs` 等宽大写，`bg-muted`；语义色只换文字色        |
| 主按钮   | `h-8 rounded-lg bg-primary text-sm font-medium`，按下 `translate-y-px`           | `Button default`：一致，`h-8` + `translate-y-px`                 |
| 次按钮   | `border border-border bg-background hover:bg-muted`                              | `Button outline`：`border-input bg-input/20`，暗色下也有可见边界 |
| 输入框   | `h-8 rounded-lg border border-input px-2.5 text-sm`，聚焦 `ring-3`               | `Input`：一致                                                    |
| 页头     | 吸顶 `bg-white/70 backdrop-blur-md`                                              | 桌面端内容区不吸顶、不模糊；仅浮层用 `Panel blur`                |

## 十、维护

- 改 token 或组件：先改本文件，再改代码，最后跑 `pnpm check:design`、`pnpm lint`、`pnpm build`。
- 新增颜色前先问「能不能用中性色 + 位置表达」；确实需要语义色时，优先复用 `success/warning/destructive/link`。
- 开发模式可访问 `?styleguide` 查看色板、排版、按钮、徽标、列表行、状态标记与面板。
- 逐页走查用 `?preview` 预览数据（`src/features/dev/fixtures.ts`），生产构建会整段摇掉。
