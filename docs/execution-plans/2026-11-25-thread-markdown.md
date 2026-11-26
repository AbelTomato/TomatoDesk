# 实施计划：PR / Issue 详情页 Markdown 渲染

- 状态：已完成（阶段 A / B 均已落地，待真实 GitHub 数据验收）
- 创建日期：2026-11-25
- 最后更新：2026-11-25
- 负责人：AbelTomato

## 目标

把 PR / Issue 详情页的正文从 `whitespace-pre-wrap` 纯文本改为按 GitHub 语义渲染 Markdown，并为评论提供同样的渲染；标题与摘要保持纯文本。渲染必须与现有中性色板、五档字号一致，且不引入 HTML 注入面。

## 背景与现状

- 详情页 `src/features/repos/ThreadDetailPage.tsx` 目前用 `<p className="… whitespace-pre-wrap">{thread.body}</p>` 输出正文，`## 改动`、`- 列表`、围栏代码块、表格都按字面量显示。
- 数据侧 `github_threads.body` 存的是 GitHub 返回的 Markdown 原文（`application/github.rs` 直接落库，未做转换），渲染层可以直接消费，不需要后端改造。
- `comment_count` 只有数量，**没有评论正文**：`infrastructure/github/mod.rs` 的 `GithubIssue` / `GithubPull` 只反序列化 `comments` 计数，没有评论接口调用，也没有评论表。要渲染评论必须先补「按需拉取 + 本地缓存」。
- 前端没有 Markdown 依赖，也没有前端测试框架；`tauri.conf.json` 的 `security.csp` 为 `null`。
- 设计约束见 `docs/design/design-language.md`：中性优先、字号只有五档、`font-mono text-xs` 承载元信息、禁止硬编码颜色与彩色填充底。`pnpm check:design` 会扫描 `src/`。

## 范围

### 包含

- 正文（PR / Issue `body`）的 Markdown 渲染。
- 评论正文的按需同步、缓存与渲染（阶段 B）。
- GFM 语法支持、原始 HTML 的安全白名单、URL 协议校验、外链走系统浏览器。
- `.markdown` 排版层与暗 / 浅主题一致性。
- 样式指南与预览数据中的 Markdown 走查样例。
- 相关文档：设计语言新增「Markdown 正文」小节、开发文档补测试命令。

### 不包含

- **标题不渲染 Markdown**。GitHub 网页端本身不渲染标题，且标题同时出现在列表行（`line-clamp`）、面包屑与复制场景，渲染会破坏截断与可访问性。
- **摘要（`summary`）不渲染**。它由后端压平生成，用于概览面板 `line-clamp-6`；渲染后无法可靠截断。
- **Release 正文、PR diff、review comments（代码行级评论）** 本次不改。Release 可复用同一组件，作为后续独立任务。
- 语法高亮、Mermaid、数学公式、`@mention` / `#123` 自动链接、图片代理与附件重写。
- 修改 `tauri.conf.json` 的 CSP（当前为 `null`，本次不引入新的注入面，不顺手改）。

## 内容范围与语法

| 位置                   | 是否渲染 | 说明                   |
| ---------------------- | -------- | ---------------------- |
| 详情页正文             | 是       | 主目标，完整 GFM       |
| 详情页评论             | 是       | 阶段 B，需先补评论同步 |
| 详情页标题             | 否       | 纯文本，与 GitHub 一致 |
| 概览面板摘要           | 否       | 纯文本 + `line-clamp`  |
| 列表行标题 / 元信息    | 否       | 纯文本                 |
| Release 正文           | 否       | 本次不动               |
| PR 变更规模 / 元信息表 | 否       | 纯文本                 |

支持语法（CommonMark + GFM）：

- 标题 `h1`–`h6`、段落、粗体 / 斜体 / 删除线、行内代码、围栏代码块（保留语言标记，不做高亮）、有序 / 无序 / 嵌套列表、任务列表（只读复选框）、引用、表格、分隔线、链接、图片、自动链接、脚注。
- 单换行按 GitHub 语义渲染为 `<br>`（`remark-breaks`），否则 issue 里常见的「一行一句」会被合并成一段。
- 原始 HTML：允许 GitHub 常用且安全的子集（`details` / `summary` / `kbd` / `sub` / `sup` / `br` / `img` / `a` / 表格相关标签），经 `rehype-sanitize` 白名单过滤后渲染。**不允许** `script`、`style`、`iframe`、`object`、`embed`、`form` 及任何 `on*` 事件属性、`style`、`class`、`id`。
- 明确降级：Mermaid 显示为代码块；`@user`、`#123` 保持纯文本；相对图片链接按 `thread.url` 解析，解析失败时显示 alt 文本。

## 安全模型

渲染链路固定为：

```
remark-parse → remark-gfm → remark-breaks → remark-rehype
→ rehype-raw → rehype-sanitize（白名单） → React 元素
```

1. **不产生 innerHTML**。`react-markdown` 输出 React 元素，全链路不使用 `dangerouslySetInnerHTML`；在 `eslint.config.js` 增加 `no-restricted-syntax` 禁止该 JSX 属性，防止后续绕过。
2. **原始 HTML 必须过 sanitize**。`rehype-raw` 只负责把 HTML 解析成 hast，安全由紧随其后的 `rehype-sanitize` 负责；禁止在任何位置插入「先 raw 后不过滤」的链路。白名单以 `defaultSchema` 为基线，按真实样本显式追加标签，属性只保留 `href` / `src` / `alt` / `title` / `colspan` / `rowspan` / `start` / `type` / `checked` / `disabled`。落地时确认三处细节：`code` 的 `className` 只放行 `language-*`（否则代码块语言标记被剥掉）、任务列表的 `input[type=checkbox][disabled]` 在白名单内、`details` / `summary` 显式加入允许标签。
3. **URL 协议白名单**。链接只允许 `http:` / `https:`（可选 `mailto:`），图片只允许 `https:`；拒绝 `javascript:`、`data:`、`vbscript:`、`file:` 与协议相对地址。相对地址先以 `thread.url` 为 base 解析，再校验协议——校验必须在解析之后，否则 `javascript:` 会被 base 掩盖。
4. **外链不劫持 WebView**。`a` 的点击统一 `preventDefault` 后交给 `openExternal`（已有 `src/lib/openExternal.ts`），不设 `target="_blank"`；锚点 `#issuecomment-…` 在应用内无对应节点，同样交给浏览器打开。
5. **图片**：仅 `https:`，`loading="lazy"`、`decoding="async"`、`referrerPolicy="no-referrer"`，`max-w-full`；不加载 `data:` URI；`<img>` 上下文中的 SVG 不执行脚本，仍限制为 https。
6. **长度兜底**：单条正文 / 评论超过 100,000 字符时截断渲染并提示「内容过长，仅显示前 10 万字符」。GitHub 自身限制 65,536 字符，这是防御异常数据而非正常路径。
7. **评论数据不放大请求**：只在打开详情时按需拉取，Rust 侧持有 Token，前端不接触凭据。

## 样式与主题

不引入 `@tailwindcss/typography`。`prose` 的默认字号阶梯、颜色与间距和「五档字号 + 中性色板」冲突，覆盖成本高于自建；自建还能保证暗 / 浅主题只依赖 CSS 变量。

在 `src/styles/index.css` 的 `@layer components` 增加 `.markdown`，全部引用现有变量：

| 元素         | 配方                                                                                                    |
| ------------ | ------------------------------------------------------------------------------------------------------- |
| 正文段落     | `text-sm leading-7`，`my-3 first:mt-0 last:mb-0`                                                        |
| h1 / h2 / h3 | `text-lg` / `text-base` / `text-sm` + `font-bold tracking-tight`；h2 加 `border-b border-border pb-1.5` |
| h4–h6        | `text-sm font-medium`                                                                                   |
| 列表         | `list-disc` / `list-decimal`，`pl-5`，嵌套 `my-1`                                                       |
| 任务列表     | 复选框 `disabled`，`accent-[var(--link)]`，去掉默认 `list-style`                                        |
| 行内代码     | `rounded bg-muted px-1 py-0.5 font-mono text-xs`                                                        |
| 代码块       | `rounded-lg border border-border bg-background p-3 font-mono text-xs leading-6 overflow-x-auto`         |
| 引用         | `border-l-2 border-border pl-3 text-muted-foreground`                                                   |
| 表格         | 外层 `overflow-x-auto`；`th` 左对齐 `font-medium`；单元格 `border-b border-border px-3 py-1.5`          |
| 链接         | `text-link underline-offset-2 hover:underline`                                                          |
| 图片         | `max-w-full rounded-lg border border-border`                                                            |
| 分隔线       | `border-border`                                                                                         |

- 表格与代码块各自横向滚动，不撑破 `AppShell` 的锁高容器，也不制造页面级横向溢出。
- 详情页正文容器保持 `max-w-3xl`，不因为 Markdown 再放宽。
- 语义色只用于链接与任务复选框，不新增颜色 token。

## 实施步骤

### 阶段 A：正文渲染（P0，纯前端）

1. [x] 安装依赖：`react-markdown`、`remark-gfm`、`remark-breaks`、`rehype-raw`、`rehype-sanitize`。均为 ESM，与 `"type": "module"` + Vite 兼容。
2. [x] 新增 `src/components/markdown/markdownUrl.ts`：`resolveMarkdownUrl(href, baseUrl)`、`isSafeLinkUrl`、`isSafeImageUrl`，纯函数、无 React 依赖，便于单测。
3. [x] 新增 `src/components/markdown/Markdown.tsx`：封装 `ReactMarkdown`，固定插件顺序与 sanitize schema，映射 `a` / `img` / `input` / `table` 等元素，链接点击走 `openExternal`；props 为 `source`、`baseUrl`、可选 `className`。
4. [x] `src/styles/index.css` 增加 `.markdown` 排版层。
5. [x] `ThreadDetailPage.tsx` 的「正文」区改用 `<Markdown>`，空态与 `SectionTitle` 保持不变。
6. [x] `StyleGuide.tsx` 增加「Markdown」样例区：标题、列表、任务列表、表格、代码块、引用、图片、`<details>`、以及四个恶意样例（`<script>`、`onerror`、`javascript:` 链接、`data:` 图片），用于暗 / 浅走查。
7. [x] `fixtures.ts` 把详情正文换成覆盖各语法的样例，并保留一条含原始 HTML 的用例；`?preview` 可直接验收。
8. [x] `eslint.config.js` 增加禁止 `dangerouslySetInnerHTML` 的规则。
9. [x] 文档：`docs/design/design-language.md` 增加「Markdown 正文」小节（字号、间距、代码块、表格、链接、禁用项）；若引入 `vitest`，在 `docs/development.md` 补 `pnpm test`。

### 阶段 B：评论（P1，前后端）

1. [x] 迁移 `src-tauri/migrations/0004_thread_comments.sql`：新建 `github_thread_comments`（`thread_id` 外键 `ON DELETE CASCADE`、`github_id`、`author`、`body`、`url`、`created_at`、`updated_at`、`synced_at`，`UNIQUE(thread_id, github_id)`，索引 `(thread_id, created_at)`）；`github_threads` 增 `comments_synced_at`；`database.rs` 注册版本 4。
2. [x] 领域层 `domain/github.rs`：新增 `GithubThreadComment`；`ThreadDetail` 增 `comments: Vec<GithubThreadComment>` 与 `comments_synced_at: Option<String>`。
3. [x] 仓储层 `github_thread.rs`：`list_comments(thread_id)`、`replace_comments(thread_id, &[GithubThreadComment])`（单事务先删后插，处理 GitHub 侧删除）、`mark_comments_synced(thread_id)`；`get()` 一并带出本地评论。
4. [x] 客户端 `infrastructure/github/mod.rs`：新增 `issue_comments(owner, name, number)`，走 `/repos/{owner}/{name}/issues/{number}/comments`（PR 的对话评论同端点），复用 `get_pages` 分页。
5. [x] 应用层：新增 `load_thread_comments(database, thread_id, force)`。本地有缓存且 `comments_synced_at >= thread.updated_at` 且 `force == false` 时直接返回；否则取 Token 与 `github_repos` 的 owner/name，拉取一次、事务替换、返回。无 Token / 网络失败时返回本地缓存并带 `stale: true`，不当作错误。
6. [x] 命令层 `commands/repos.rs`：新增异步命令 `refresh_thread_comments`，`lib.rs` 注册；`get_thread` 继续返回本地评论，避免打开详情时同步等待网络。
7. [x] `api.ts`：补 `ThreadComment` 类型、`ThreadDetail.comments`、`refreshThreadComments(threadId, force)`。
8. [x] `ThreadDetailPage.tsx` 新增「评论」区：作者 + 相对时间 + Markdown 正文；首屏渲染本地缓存，随后在 `commentCount > comments.length` 或 `commentsSyncedAt` 为空时自动刷新一次；加载骨架、失败降级为「评论需要联网加载，去 GitHub 查看」+ 重试按钮。
9. [x] 边界：评论为空但 `commentCount > 0` 时提示「评论尚未同步」；超过 50 条先渲染 50 条 + 「加载更多」（本地切片，不重复请求）。

## 涉及文件与模块

- `package.json` / `pnpm-lock.yaml`：新增 Markdown 依赖（阶段 A）；`vitest`（可选）。
- `src/components/markdown/Markdown.tsx`（新增）：渲染组件与元素映射。
- `src/components/markdown/markdownUrl.ts`（新增）：URL 解析与协议白名单。
- `src/styles/index.css`：`.markdown` 排版层。
- `src/features/repos/ThreadDetailPage.tsx`：正文与评论区。
- `src/features/repos/RepoDetail.tsx`：仅确认摘要 / 标题保持纯文本，不改渲染。
- `src/features/dev/StyleGuide.tsx`、`src/features/dev/fixtures.ts`：走查样例。
- `src/lib/api.ts`、`src/lib/openExternal.ts`：评论类型与外部打开复用。
- `eslint.config.js`：禁止 `dangerouslySetInnerHTML`。
- `src-tauri/migrations/0004_thread_comments.sql`（新增）、`database.rs`：评论表与迁移版本。
- `src-tauri/src/domain/github.rs`、`infrastructure/db/repositories/github_thread.rs`、`infrastructure/github/mod.rs`、`application/github.rs`、`commands/repos.rs`、`lib.rs`：评论拉取与缓存。
- `docs/design/design-language.md`、`docs/development.md`：规范与命令。

## 验证方式

- 前端：`pnpm lint`、`pnpm build`（含 `tsc`）、`pnpm check:design`、`pnpm format:check` 全部通过。
- 单测（建议引入 `vitest`，只覆盖安全关键函数）：`javascript:`、`data:`、`vbscript:`、`file:`、`//evil.example`、`../x`、`#issuecomment-1`、正常 https 链接与图片，断言解析结果与放行 / 拒绝。
- Rust：`cargo fmt --check`、`cargo clippy --locked --all-targets -- -D warnings`、`cargo test --locked`；补迁移与仓储测试（版本 4、同 `github_id` 去重、按 `created_at` 升序、线程删除级联、重复同步不产生重复行）。
- 人工走查：
  - `?styleguide` 与 `?preview` 下暗 / 浅双主题截图，核对字号、间距、代码块、表格、引用与设计语言一致。
  - 恶意样例：`<script>alert(1)</script>` 不执行；`<img src=x onerror=alert(1)>` 不触发；`[x](javascript:alert(1))` 不可点击；`![](data:image/svg+xml,…)` 不加载；`<iframe>` 被丢弃。
  - 外链点击打开系统浏览器，应用内不发生导航；`#issuecomment-…` 同样外开。
  - 760 / 1280 / 1920 三档无页面级横向溢出；代码块与表格在自身容器内横向滚动；长正文在锁高容器内正常滚动。
  - 评论：首次打开触发一次请求、再次打开命中缓存、断网或无 Token 时降级提示且保留本地评论、GitHub 侧删除评论后重新同步本地不再显示。
- 真实数据：挑 3 个含 `<details>`、表格、图片、超长代码块的 issue / PR 与网页端对照；记录一次评论拉取的请求数与限流表现。

## 风险与回滚

- 风险：依赖体积上升（`react-markdown` + remark/rehype 系约百 KB 级 gzip）。桌面端可接受，构建后对比 `dist/assets` 体积；超预期则把 Markdown 组件改为 `React.lazy` 按需加载，或退回 `marked` + `dompurify`（仍必须 sanitize）。
- 风险：长正文与几十条评论带来渲染卡顿。用 `useMemo` 按 `source` 缓存渲染结果，评论分片渲染；必要时再引入虚拟列表，不在第一版做。
- 风险：白名单过严导致 `<details>` 内内容缺失或样式丢失。按真实样本补标签，**不放宽 sanitize**；任何「先关掉 sanitize 看看」的做法都视为不可接受。
- 风险：评论接口放大请求或触发限流。仅详情页触发 + `comments_synced_at` 缓存 + 手动重试；错误信息复用现有 `GithubError::RateLimited` 文案。
- 风险：相对图片链接解析到错误地址。接受降级为 alt 文本，后续再评估按仓库 default branch 重写。
- 回滚：阶段 A 只影响渲染层，删除 `Markdown` 组件与依赖即可回到 `whitespace-pre-wrap`；阶段 B 迁移只新增表与列，回退代码后旧库仍可正常读写，必要时删除新表与 `comments_synced_at` 列。

## 完成标准

- [x] 详情页正文与评论按 GFM 渲染，标题与摘要保持纯文本。
- [x] 恶意样例无脚本执行、无危险协议导航，外链一律走系统浏览器。
- [x] 暗 / 浅主题、五档字号与设计语言一致，长内容不破坏锁高与横向布局。
- [x] 评论按需加载、可离线降级、不重复请求，删除的评论不再出现。
- [x] `pnpm lint` / `build` / `check:design` / `test` 与改动文件 Prettier 检查通过；Rust `fmt` / `clippy` / `test` 通过，新增测试覆盖安全函数与评论仓储。
- [x] 设计语言、开发文档、本计划进度与当天开发日志已补齐。

## 已确认决策

1. 评论纳入本次范围，按阶段 B 做「按需拉取 + 本地缓存 + 离线降级」。
2. 启用 `rehype-raw` + `rehype-sanitize` 白名单，保留 `<details>` 等 GitHub 常用 HTML。
3. 引入 `vitest`，只覆盖 URL 协议白名单等纯函数。

## 进度记录

- 2026-11-25 21:58：完成现状调研与方案设计，计划待确认；未改动代码。
- 2026-11-25 22:12：阶段 A 完成。新增 `components/markdown/{Markdown.tsx,markdownUrl.ts}`、`.markdown` 排版层、详情页正文接入、样式指南样例与 ESLint 禁用 `dangerouslySetInnerHTML`；`vitest` 与 6 条 URL 白名单单测通过；`pnpm build` / `lint` / `check:design` 通过。
- 2026-11-25 22:35：阶段 B 完成。新增 `0004_thread_comments.sql`（`github_thread_comments` + `comments_synced_at`）、评论仓储与 `issue_comments` 客户端、`refresh_thread_comments` 命令、前端评论区与离线降级；Rust 测试 29/29 通过。
- 2026-11-25 22:40：暗 / 浅截图走查评论区与 Markdown 样例；DOM 断言 `scripts=0 / iframes=0 / 事件属性=0 / 危险协议=0 / data 图片=0`，任务列表 2 个复选框、1 个已勾选，`details` 保留。
- 待办：真实 Token 下挑 3 个 issue / PR 与网页端对照，记录一次评论拉取的请求数与限流表现。
