import type {
  AppInfo,
  CaptureBody,
  CaptureCounts,
  CapturePage,
  CapturePageInput,
  CaptureSummary,
  GithubRelease,
  GithubRepo,
  GithubSyncRun,
  MigrationNotice,
  RepoDetail,
  RepoSummary,
  Tag,
  ThreadComment,
  ThreadComments,
  ThreadDetail,
  ThreadPage,
  ThreadPageInput,
  ThreadSummary,
} from "../../lib/api";

/**
 * 开发模式下的界面走查数据：只在 `?preview` 下由 `lib/api.ts` 顶替 IPC 返回。
 * 用途是逐页核对排版与状态色，不参与任何真实读写。
 */

const minutesAgo = (minutes: number) => new Date(Date.now() - minutes * 60_000).toISOString();
const hoursAgo = (hours: number) => minutesAgo(hours * 60);
const daysAgo = (days: number) => hoursAgo(days * 24);

const repoId = "repo-octop";

const tags: Tag[] = [
  { id: "tag-rust", name: "rust", color: null, createdAt: daysAgo(9) },
  { id: "tag-design", name: "设计", color: null, createdAt: daysAgo(8) },
  { id: "tag-reading", name: "待读", color: null, createdAt: daysAgo(7) },
  { id: "tag-sqlite", name: "sqlite", color: null, createdAt: daysAgo(6) },
];

const captures: CaptureSummary[] = [
  {
    id: "capture-1",
    kind: "note",
    source: "quick-capture",
    url: null,
    title: "把列表与正文拆成两级加载",
    summary: "首屏只取索引字段，正文点开再取；同步时摘要与正文分写，避免同一段内容存两遍。",
    note: null,
    repoId: null,
    archivedAt: null,
    createdAt: hoursAgo(5),
    updatedAt: minutesAgo(24),
  },
  {
    id: "capture-2",
    kind: "link",
    source: "quick-capture",
    url: "https://github.com/TencentCloud/Octop",
    title: "Octop：GitHub 事件汇总工具",
    summary: "把 Issue、PR、Release 与评审请求聚合成一条时间线，适合做仓库工作区的数据来源。",
    note: "先看它怎么处理增量同步与 ETag。",
    repoId: null,
    archivedAt: null,
    createdAt: hoursAgo(9),
    updatedAt: hoursAgo(2),
  },
  {
    id: "capture-3",
    kind: "note",
    source: "quick-capture",
    url: null,
    title: "SQLite FTS5 的中文分词",
    summary:
      "默认 unicode61 分词对中文不友好，可以退化为 bigram，或者只在正文上建索引配合 LIKE 兜底。",
    note: null,
    repoId: null,
    archivedAt: null,
    createdAt: daysAgo(1),
    updatedAt: daysAgo(1),
  },
  {
    id: "capture-4",
    kind: "link",
    source: "quick-capture",
    url: "https://tailwindcss.com/docs/theme",
    title: "Tailwind v4 的 @theme 与 CSS 变量",
    summary:
      "在 @theme inline 里把语义变量映射成工具类，组件只写 bg-card / text-muted-foreground 这类名字。",
    note: null,
    repoId: null,
    archivedAt: null,
    createdAt: daysAgo(2),
    updatedAt: daysAgo(2),
  },
  {
    id: "capture-5",
    kind: "note",
    source: "quick-capture",
    url: null,
    title: "WebView2 下 backdrop-filter 的开销",
    summary: "大面积模糊会让滚动掉帧，只适合吸顶栏、弹窗与浮层；列表与面板保持不透明。",
    note: null,
    repoId: null,
    archivedAt: daysAgo(4),
    createdAt: daysAgo(4),
    updatedAt: daysAgo(4),
  },
  {
    id: "capture-6",
    kind: "link",
    source: "quick-capture",
    url: "https://tauri.app/plugin/updater/",
    title: "Tauri v2 Updater 与签名清单",
    summary: "latest.json 里放各平台的签名与下载地址，客户端校验签名后再交给安装程序。",
    note: null,
    repoId: null,
    archivedAt: null,
    createdAt: daysAgo(5),
    updatedAt: daysAgo(3),
  },
];

const bodies: Record<string, string> = {
  "capture-1":
    "改造分三步：先给 captures 增 body 列并把旧 summary 迁进去，再把列表接口裁成索引字段，最后让详情按需取正文。\n\n分页走键集游标，格式是「排序值|id」，对前端不透明。计数下沉 SQL，避免把全部行拉到 JS 里算。",
  "capture-2":
    "Octop 的做法是先用 state=all 分页拉全量，再在本地按仓库聚合。这样第一次同步很重，但后续可以靠 since 只取改动。\n\n需要注意：Issue 与 PR 在 GitHub API 里共用 /issues 端点，PR 会多出 pull_request 字段，要按这个字段分流。",
  "capture-3":
    "FTS5 的 unicode61 分词器把连续汉字当成一个 token，导致「仓库工作区」搜不到「仓库」。\n\n两种绕法：\n1. 建索引时用 bigram 分词（tokenize='unicode61' 无法直接做到，需要 trigram）。\n2. 保持 FTS 只覆盖英文与代码标识符，中文走 LIKE 兜底。",
  "capture-4":
    "@theme inline 的意义是把 CSS 变量映射成工具类时不做二次求值，变量在 :root 里随主题切换，工具类自动跟着变。\n\n所以暗色不需要给每个组件写 dark: 变体，只要换 :root 的变量值。",
  "capture-5":
    "实测在 1280×800 的窗口里，列表项带 backdrop-blur 时滚动帧率从 60 掉到 40 左右。\n\n结论：只在固定不滚动的区域使用。",
  "capture-6":
    "签名用 minisign 私钥签，公钥写进 tauri.conf.json 的 plugins.updater.pubkey。CI 里私钥走 Secrets，签完把 latest.json 一起发到 Release。",
};

const captureTags: Record<string, Tag[]> = {
  "capture-1": [tags[0], tags[3]],
  "capture-2": [tags[2]],
  "capture-3": [tags[3]],
};

const counts: CaptureCounts = { all: 1284, active: 1214, archived: 70 };

const repos: RepoSummary[] = [
  {
    id: repoId,
    owner: "TencentCloud",
    name: "Octop",
    description: "把 GitHub 通知与事件聚合成时间线的桌面工具。",
    language: "Rust",
    stars: 2143,
    openPullRequests: 6,
    openIssues: 24,
    reviews: 3,
    releases: 12,
    captures: 4,
    enabled: true,
    lastSyncedAt: minutesAgo(18),
    isArchived: false,
    isPrivate: false,
  },
  {
    id: "repo-blog",
    owner: "AbelTomato",
    name: "AbelTomato.github.io",
    description: "个人博客：Astro + Tailwind v4，暗色优先的中性色板。",
    language: "Astro",
    stars: 86,
    openPullRequests: 1,
    openIssues: 2,
    reviews: 0,
    releases: 4,
    captures: 7,
    enabled: true,
    lastSyncedAt: hoursAgo(6),
    isArchived: false,
    isPrivate: false,
  },
  {
    id: "repo-tauri",
    owner: "tauri-apps",
    name: "tauri",
    description: "构建更小、更快、更安全的桌面应用。",
    language: "Rust",
    stars: 84_200,
    openPullRequests: 32,
    openIssues: 118,
    reviews: 5,
    releases: 61,
    captures: 0,
    enabled: true,
    lastSyncedAt: daysAgo(2),
    isArchived: false,
    isPrivate: false,
  },
  {
    id: "repo-legacy",
    owner: "AbelTomato",
    name: "TomatoDesk-legacy",
    description: null,
    language: "TypeScript",
    stars: 3,
    openPullRequests: 0,
    openIssues: 0,
    reviews: 0,
    releases: 0,
    captures: 0,
    enabled: false,
    lastSyncedAt: null,
    isArchived: true,
    isPrivate: true,
  },
];

const repo: RepoDetail = {
  ...repos[0],
  etag: null,
  homepage: "https://github.com/TencentCloud/Octop",
  forks: 132,
  openIssues: 24,
  defaultBranch: "main",
  pushedAt: hoursAgo(3),
  readmeText:
    "# Octop\n\n把 GitHub 通知与事件聚合成时间线。\n\n## 特性\n\n- 按仓库组织 Issue、PR 与 Release\n- 本地 SQLite 存储，离线可读\n- 增量同步，只拉最近改动\n",
  readmeSummary:
    "把 GitHub 通知与事件聚合成时间线。特性：按仓库组织 Issue、PR 与 Release；本地 SQLite 存储，离线可读；增量同步，只拉最近改动。",
  readmeSyncedAt: minutesAgo(18),
  customDescription: "我的仓库工作区数据来源，重点看增量同步的实现。",
  customNote: "分支约定：main 保护，功能分支走 feature/*。\n发布节奏：每月一个小版本。",
  metaSyncedAt: minutesAgo(18),
  createdAt: daysAgo(30),
  updatedAt: minutesAgo(18),
  counts: { openPullRequests: 6, openIssues: 24, reviews: 3, releases: 12, captures: 4 },
};

const threads: ThreadSummary[] = [
  {
    id: "thread-1",
    repoId,
    kind: "pull_request",
    number: 482,
    title: "feat: 用键集游标替换 offset 分页",
    state: "open",
    author: "AbelTomato",
    url: "https://github.com/TencentCloud/Octop/pull/482",
    summary: "把列表接口换成键集分页，顺手把计数下沉到 SQL。",
    isAuthor: true,
    isAssigned: false,
    reviewRequested: false,
    reviewed: false,
    commentCount: 4,
    additions: 386,
    deletions: 212,
    changedFiles: 11,
    createdAt: daysAgo(3),
    updatedAt: minutesAgo(12),
    closedAt: null,
    mergedAt: null,
  },
  {
    id: "thread-2",
    repoId,
    kind: "pull_request",
    number: 479,
    title: "fix: 同步时不再重复写入正文",
    state: "merged",
    author: "someone-else",
    url: "https://github.com/TencentCloud/Octop/pull/479",
    summary: "payload_json 与 summary 各存一份正文，去掉其中一份。",
    isAuthor: false,
    isAssigned: false,
    reviewRequested: false,
    reviewed: true,
    commentCount: 9,
    additions: 64,
    deletions: 188,
    changedFiles: 5,
    createdAt: daysAgo(6),
    updatedAt: daysAgo(1),
    closedAt: daysAgo(1),
    mergedAt: daysAgo(1),
  },
  {
    id: "thread-3",
    repoId,
    kind: "pull_request",
    number: 476,
    title: "chore: 升级到 Tauri 2.1",
    state: "draft",
    author: "someone-else",
    url: "https://github.com/TencentCloud/Octop/pull/476",
    summary: "还在等上游修 WebView2 的拖拽问题。",
    isAuthor: false,
    isAssigned: false,
    reviewRequested: true,
    reviewed: false,
    commentCount: 2,
    additions: 21,
    deletions: 18,
    changedFiles: 3,
    createdAt: daysAgo(8),
    updatedAt: hoursAgo(20),
    closedAt: null,
    mergedAt: null,
  },
  {
    id: "thread-4",
    repoId,
    kind: "pull_request",
    number: 470,
    title: "refactor: 把 GitHub 客户端拆成独立模块",
    state: "closed",
    author: "someone-else",
    url: "https://github.com/TencentCloud/Octop/pull/470",
    summary: "方向不对，改为在 infrastructure 下分文件。",
    isAuthor: false,
    isAssigned: true,
    reviewRequested: false,
    reviewed: false,
    commentCount: 6,
    additions: 512,
    deletions: 40,
    changedFiles: 17,
    createdAt: daysAgo(14),
    updatedAt: daysAgo(9),
    closedAt: daysAgo(9),
    mergedAt: null,
  },
  {
    id: "thread-5",
    repoId,
    kind: "issue",
    number: 466,
    title: "两千条内容时首屏明显卡顿",
    state: "open",
    author: "someone-else",
    url: "https://github.com/TencentCloud/Octop/issues/466",
    summary: "列表一次性取回全部行，滚动时还要在 JS 里排序。",
    isAuthor: false,
    isAssigned: true,
    reviewRequested: false,
    reviewed: false,
    commentCount: 11,
    additions: null,
    deletions: null,
    changedFiles: null,
    createdAt: daysAgo(20),
    updatedAt: hoursAgo(4),
    closedAt: null,
    mergedAt: null,
  },
  {
    id: "thread-6",
    repoId,
    kind: "issue",
    number: 451,
    title: "希望按仓库查看自己的笔记",
    state: "closed",
    author: "AbelTomato",
    url: "https://github.com/TencentCloud/Octop/issues/451",
    summary: "已由仓库工作区的「相关随手记」满足。",
    isAuthor: true,
    isAssigned: false,
    reviewRequested: false,
    reviewed: false,
    commentCount: 3,
    additions: null,
    deletions: null,
    changedFiles: null,
    createdAt: daysAgo(26),
    updatedAt: daysAgo(12),
    closedAt: daysAgo(12),
    mergedAt: null,
  },
];

const threadCommentBodies: Record<string, string[]> = {
  "thread-1": [
    "游标格式建议写成不透明字符串，前端不要解析它。\n\n`排序值|id` 里的 `|` 如果出现在值里会截断，记得转义或改用 base64。",
    "两千条数据下首屏 50 条够用吗？我这边滚动到底大概要 **6 秒**。\n\n- 可以试试把 `limit` 提到 80\n- 或者先加个骨架屏",
    "> 计数下沉到 SQL 之后，`count_captures` 一次大概 3ms。\n\n可以接受。",
    "补了个压测脚本，见 `scripts/bench.mjs`。\n\n```bash\npnpm bench --rows=2000\n```",
  ],
  "thread-5": [
    "确认一下：慢的是首屏请求还是渲染？\n\n如果是渲染，先看列表是不是一次性挂了两千个节点。",
    "渲染。改成 `divide-y` + 分页之后好多了。",
    "已合入，等下一个版本验证。",
  ],
};

const commentsForThread = (id: string): ThreadComment[] => {
  const thread = threads.find((entry) => entry.id === id);
  const bodies = threadCommentBodies[id] ?? [];
  if (!thread) return [];
  return bodies.map((body, index) => ({
    id: `${id}-comment-${index + 1}`,
    threadId: id,
    githubId: 1000 + index,
    author: index % 2 === 0 ? "someone-else" : "AbelTomato",
    body,
    url: `${thread.url}#issuecomment-${1000 + index}`,
    createdAt: minutesAgo(200 - index * 20),
    updatedAt: minutesAgo(200 - index * 20),
    syncedAt: minutesAgo(12),
  }));
};

const threadDetail = (id: string): ThreadDetail | null => {
  const thread = threads.find((entry) => entry.id === id);
  if (!thread) return null;
  const comments = commentsForThread(id);
  return {
    ...thread,
    // 详情里的评论数与样例评论对齐，避免走查时数字和列表对不上。
    commentCount: comments.length,
    comments,
    commentsSyncedAt: comments.length > 0 ? minutesAgo(12) : null,
    body:
      thread.kind === "pull_request"
        ? [
            "## 改动",
            "",
            "把列表接口换成键集分页，顺手把计数下沉到 SQL。",
            "",
            "- 新增 `list_captures_page`，游标格式 `排序值|id`",
            "- 计数走 `count_captures`，不再把全部行拉到 JS 里算",
            "- 详情正文按需加载",
            "",
            "### 关键代码",
            "",
            "```rust",
            "let page = repo.page(&query)?;",
            "```",
            "",
            "| 项目 | 改造前 | 改造后 |",
            "| --- | --- | --- |",
            "| 首屏条数 | 全部 | 50 |",
            "| 计数 | 前端聚合 | SQL |",
            "",
            "### 待办",
            "",
            "- [x] 迁移脚本",
            "- [ ] 补两千条数据的压测",
            "",
            "> 注意：`open` 筛选包含草稿 PR。",
            "",
            "<details>",
            "<summary>展开看验证记录</summary>",
            "",
            "两千条数据下首屏只取 50 条，滚动时增量加载；长列表不再一次性渲染。",
            "",
            "</details>",
            "",
            "参考 [键集分页](https://example.com/keyset)，或直接看 `src/lib/api.ts`。",
          ].join("\n")
        : [
            "## 现象",
            "",
            "Inbox 打开要等两三秒，滚动也发涩。",
            "",
            "## 期望",
            "",
            "首屏只渲染一页，剩下的滚动加载。",
            "",
            "<details>",
            "<summary>补充信息</summary>",
            "",
            "- 数据量约 2000 条",
            "- 关掉标签筛选后仍然卡",
            "",
            "</details>",
          ].join("\n"),
  };
};

const releases: GithubRelease[] = [
  {
    id: "release-1",
    repoId,
    tagName: "v0.3.0",
    name: "仓库工作区",
    url: "https://github.com/TencentCloud/Octop/releases/tag/v0.3.0",
    body: "### 新增\n\n- 仓库工作区：按仓库查看 PR、Issue、评审、Release 与相关随手记\n- 概览支持自定义描述与备注，行内编辑、失焦保存\n\n### 变更\n\n- GitHub 数据从随手记迁出，收件箱回归外部消息处理\n- 同步改为整仓一个事务，逐列比较后才写库",
    summary: "新增仓库工作区与概览自定义描述；GitHub 数据从随手记迁出。",
    isDraft: false,
    isPrerelease: false,
    publishedAt: daysAgo(2),
    createdAt: daysAgo(2),
    syncedAt: minutesAgo(18),
  },
  {
    id: "release-2",
    repoId,
    tagName: "v0.2.0",
    name: "应用内更新",
    url: "https://github.com/TencentCloud/Octop/releases/tag/v0.2.0",
    body: "签名清单 + 应用内更新，安装包体积控制在 12 MB 以内。",
    summary: "签名清单与应用内更新。",
    isDraft: false,
    isPrerelease: false,
    publishedAt: daysAgo(20),
    createdAt: daysAgo(20),
    syncedAt: minutesAgo(18),
  },
  {
    id: "release-3",
    repoId,
    tagName: "v0.4.0-rc.1",
    name: "视觉收尾",
    url: "https://github.com/TencentCloud/Octop/releases/tag/v0.4.0-rc.1",
    body: "设计语言文档、页面级重排与状态色例外。",
    summary: "设计语言文档与页面级重排。",
    isDraft: false,
    isPrerelease: true,
    publishedAt: hoursAgo(6),
    createdAt: hoursAgo(6),
    syncedAt: minutesAgo(18),
  },
];

const githubRepos: GithubRepo[] = repos.slice(0, 3).map((entry, index) => ({
  ...repo,
  id: entry.id,
  owner: entry.owner,
  name: entry.name,
  enabled: entry.enabled,
  lastSyncedAt: entry.lastSyncedAt,
  etag: null,
  description: entry.description,
  language: entry.language,
  stars: entry.stars,
  isArchived: entry.isArchived,
  isPrivate: entry.isPrivate,
  customDescription: index === 0 ? repo.customDescription : null,
  customNote: index === 0 ? repo.customNote : null,
}));

const syncRuns: GithubSyncRun[] = [
  {
    id: "run-1",
    source: "github",
    startedAt: minutesAgo(18),
    finishedAt: minutesAgo(17),
    status: "succeeded",
    error: null,
  },
  {
    id: "run-2",
    source: "github",
    startedAt: hoursAgo(6),
    finishedAt: hoursAgo(6),
    status: "succeeded",
    error: null,
  },
  {
    id: "run-3",
    source: "github",
    startedAt: daysAgo(1),
    finishedAt: daysAgo(1),
    status: "failed",
    error: "GitHub API 返回 403：rate limit exceeded。",
  },
];

const pageFor = (input: CapturePageInput): CapturePage => {
  const filtered = captures
    .filter((capture) => input.archived == null || Boolean(capture.archivedAt) === input.archived)
    .filter((capture) => !input.kind || capture.kind === input.kind)
    .filter((capture) => !input.repoId || capture.repoId === input.repoId);
  return { captures: filtered, nextCursor: null, hasMore: false };
};

const threadsFor = (input: ThreadPageInput): ThreadPage => {
  const filtered = threads
    .filter((thread) => thread.kind === input.kind)
    // 与后端一致：open 把草稿也算进去（草稿 PR 是打开但未就绪）。
    .filter((thread) =>
      !input.state
        ? true
        : input.state === "open"
          ? thread.state === "open" || thread.state === "draft"
          : thread.state === input.state,
    )
    .filter((thread) => {
      if (!input.relationship || input.relationship === "none") return true;
      if (input.relationship === "author") return thread.isAuthor;
      if (input.relationship === "assigned") return thread.isAssigned;
      if (input.relationship === "review_requested") return thread.reviewRequested;
      return thread.reviewed;
    });
  return { threads: filtered, nextCursor: null, hasMore: false };
};

const bodyFor = (id: string): CaptureBody => {
  const capture = captures.find((entry) => entry.id === id);
  return {
    id,
    kind: capture?.kind ?? "note",
    url: capture?.url ?? null,
    body: bodies[id] ?? null,
  };
};

const commentsFor = (id: string): ThreadComments => {
  const comments = commentsForThread(id);
  return {
    comments,
    syncedAt: comments.length > 0 ? minutesAgo(12) : null,
    stale: false,
    error: null,
  };
};

const appInfo: AppInfo = { name: "TomatoDesk", version: "0.3.0", environment: "development" };
const migrationNotice: MigrationNotice = { migratedTo: null, backupPath: null, message: null };

export const fixtures = {
  counts,
  tags,
  captures,
  captureTags,
  repos,
  repo,
  threads,
  releases,
  githubRepos,
  syncRuns,
  appInfo,
  migrationNotice,
  pageFor,
  threadsFor,
  bodyFor,
  detailFor: threadDetail,
  commentsFor,
  search: (query: string) =>
    captures.filter((capture) =>
      [capture.title, capture.summary, capture.note, capture.source]
        .filter(Boolean)
        .some((field) => field!.toLowerCase().includes(query.toLowerCase())),
    ),
};
