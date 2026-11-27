import { invoke } from "@tauri-apps/api/core";

export type CommandErrorCode = "validation" | "not_found" | "conflict" | "internal";

export interface CommandErrorPayload {
  code: CommandErrorCode;
  message: string;
}

export class ApiError extends Error {
  constructor(
    readonly code: CommandErrorCode,
    message: string,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

export interface AppInfo {
  name: string;
  version: string;
  environment: "development" | "production";
}

/** 本次启动执行了数据库迁移时的提示，`migratedTo` 为空表示无需提示。 */
export interface MigrationNotice {
  migratedTo: number | null;
  backupPath: string | null;
  message: string | null;
}

export type CaptureKind = "note" | "link";
export type CaptureSort = "updated" | "created";

/** 列表行：不含正文，首屏只传索引字段。 */
export interface CaptureSummary {
  id: string;
  kind: CaptureKind;
  source: string;
  url: string | null;
  title: string | null;
  summary: string | null;
  note: string | null;
  /** 挂在某个仓库下时指向该仓库。 */
  repoId: string | null;
  /** 归档时间；null 表示未归档。 */
  archivedAt: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface Capture extends CaptureSummary {
  body: string | null;
}

export interface CapturePage {
  captures: CaptureSummary[];
  nextCursor: string | null;
  hasMore: boolean;
}

export interface CapturePageInput {
  /** undefined / null 表示全部；false 只看未归档；true 只看已归档。 */
  archived?: boolean | null;
  kind?: CaptureKind | null;
  repoId?: string | null;
  sort?: CaptureSort;
  cursor?: string | null;
  limit?: number;
}

export interface CaptureCounts {
  all: number;
  active: number;
  archived: number;
}

export interface CaptureBody {
  id: string;
  kind: CaptureKind;
  url: string | null;
  body: string | null;
}

export interface CreateCaptureInput {
  kind: CaptureKind;
  source: string;
  url: string | null;
  title: string | null;
  summary: string | null;
  note: string | null;
  /** 完整正文；笔记类由调用方写入，链接类通常留空等回源抓取。 */
  body: string | null;
  repoId: string | null;
}

export interface QuickCaptureInput {
  value: string;
  note: string | null;
}

/** 更新只改内容；归档状态走 archive / restore，仓库关联在创建时确定。 */
export interface UpdateCaptureInput {
  id: string;
  kind: CaptureKind;
  source: string;
  url: string | null;
  title: string | null;
  summary: string | null;
  note: string | null;
}

/**
 * 列表行转成更新入参：后端拒绝未知字段，所以只挑 DTO 里的字段，
 * 不能直接展开 `CaptureSummary`（会带上 createdAt / updatedAt / repoId / archivedAt）。
 */
export function toUpdateCaptureInput(capture: CaptureSummary): UpdateCaptureInput {
  return {
    id: capture.id,
    kind: capture.kind,
    source: capture.source,
    url: capture.url,
    title: capture.title,
    summary: capture.summary,
    note: capture.note,
  };
}

export interface Tag {
  id: string;
  name: string;
  color: string | null;
  createdAt: string;
}

export interface CreateTagInput {
  name: string;
  color: string | null;
}

export type TaskStatus = "open" | "done" | "cancelled";

/** 待办字段都是短文本，列表与详情共用同一个投影，没有单列 summary 类型。 */
export interface Task {
  id: string;
  title: string;
  note: string | null;
  status: TaskStatus;
  /** `manual` / `inbox` / `capture`；手动创建为 `manual`。 */
  originKind: string | null;
  originId: string | null;
  /** 来源消息的消息源类型；列表行据此把「来自收件箱」写细，非收件箱来源为空。 */
  originSourceKind: InboxSourceKind | null;
  createdAt: string;
  updatedAt: string;
  /** 完成时间；取消或重开会清空。 */
  completedAt: string | null;
}

export interface TaskPage {
  tasks: Task[];
  nextCursor: string | null;
  hasMore: boolean;
}

export interface TaskPageInput {
  /** undefined / null 表示全部。 */
  status?: TaskStatus | null;
  cursor?: string | null;
  limit?: number;
}

export interface TaskCounts {
  open: number;
  done: number;
  cancelled: number;
  all: number;
}

export interface CreateTaskInput {
  title: string;
  note?: string | null;
}

export interface UpdateTaskInput {
  id: string;
  title: string;
  note: string | null;
}

export type InboxSourceKind = "github" | "email" | "rss" | "manual";
export type TriageOutcome = "dismissed" | "task" | "capture";

/** 收件箱状态轴：四个 Tab 互斥且穷尽，结果本身就是 Tab。 */
export type InboxTab = "pending" | "task" | "dismissed" | "all";

export interface InboxSource {
  id: string;
  kind: InboxSourceKind;
  name: string;
  enabled: boolean;
  lastSyncedAt: string | null;
  lastError: string | null;
  createdAt: string;
  updatedAt: string;
}

/** 一条收件箱消息：原文不可编辑，用户只能改已读与判断结果。 */
export interface InboxMessage {
  id: string;
  sourceId: string;
  externalId: string;
  /** mention / review_requested / assigned / ci_failed / release / mail */
  category: string;
  title: string;
  summary: string | null;
  body: string | null;
  url: string | null;
  /** 触发人 / 发件人。 */
  actor: string | null;
  occurredAt: string;
  /** 看过时间；不等于判断过。 */
  readAt: string | null;
  triagedAt: string | null;
  triageOutcome: TriageOutcome | null;
  triageRefId: string | null;
  /** `null` 是人处理的，`rule` 是规则自动处理的。 */
  triageBy: "rule" | null;
  relatedKind: string | null;
  relatedId: string | null;
}

export interface InboxPage {
  messages: InboxMessage[];
  nextCursor: string | null;
  hasMore: boolean;
  /** 当前筛选下的总条数，只用来给「选择全部 N 条」报数。 */
  total: number;
}

export interface InboxPageInput {
  /** 缺省为 `pending`（待处理）。 */
  tab?: InboxTab | null;
  unreadOnly?: boolean;
  category?: string | null;
  sourceId?: string | null;
  cursor?: string | null;
  limit?: number;
}

/** 计数只随来源与分类变化：切 Tab 时四个数字必须还能相加等于「全部」。 */
export interface InboxCountFilter {
  sourceId?: string | null;
  category?: string | null;
}

export interface InboxCounts {
  all: number;
  pending: number;
  task: number;
  dismissed: number;
  /** 与「只看未读」同一口径：`readAt` 为空，不要求未处理。 */
  unread: number;
}

/**
 * 批量动作：单条与批量走同一条命令，撤销就是它的反向。
 * 不再区分「一键处理」，动作本身才是唯一的入口。
 */
export type InboxAction = "read" | "unread" | "dismiss" | "task";

/** 「选择全部 N 条」用的筛选：与列表查询同一套条件，避免两处漂移。 */
export interface InboxScopeFilter {
  tab?: InboxTab | null;
  unreadOnly?: boolean;
  category?: string | null;
  sourceId?: string | null;
}

export interface ApplyInboxActionInput {
  action: InboxAction;
  /** 显式勾选的 id；与 `scope` 二选一。 */
  ids?: string[] | null;
  /** 跨页「选择全部」时由后端按筛选解析 id。 */
  scope?: InboxScopeFilter | null;
  /** `scope` 模式必填：与实际条数不符时不执行，返回值里带实际数字。 */
  expectedCount?: number | null;
}

export interface UndoInboxActionInput {
  /** 当初执行的动作，不是要执行的反向动作。 */
  action: InboxAction;
  ids: string[];
  taskIds?: string[];
}

export interface InboxActionResult {
  action: InboxAction;
  /** 真正被改动的消息：重复点击或已被别处处理的行不会出现在这里。 */
  appliedIds: string[];
  /** 转待办生成的待办，撤销时用于解绑。 */
  taskIds: string[];
  skipped: number;
  /** 非 `null` 表示条数与确认时不一致、一条都没改，值是实际条数。 */
  staleCount: number | null;
}

/** 规则能做的动作：转待办会生成实体，不适合在没人看的时候静默执行。 */
export type InboxRuleAction = "read" | "dismiss";

/** 一条收件箱规则：条件之间是「且」，至少有一个条件。 */
export interface InboxRule {
  id: string;
  sourceId: string | null;
  category: string | null;
  /** 发件人 / 触发人包含这段文字。 */
  actorContains: string | null;
  titleContains: string | null;
  action: InboxRuleAction;
  enabled: boolean;
  hitCount: number;
  lastHitAt: string | null;
  createdAt: string;
  updatedAt: string;
}

/** 保存规则：`id` 为空表示新建，否则更新条件与开关。 */
export interface InboxRuleInput {
  id?: string | null;
  sourceId?: string | null;
  category?: string | null;
  actorContains?: string | null;
  titleContains?: string | null;
  action: InboxRuleAction;
  enabled?: boolean | null;
}

/** 试跑入参：只要条件，不需要动作与开关。 */
export interface InboxRuleFilterInput {
  sourceId?: string | null;
  category?: string | null;
  actorContains?: string | null;
  titleContains?: string | null;
}

/** 试跑结果：命中数含已处理的行，待处理数才是规则真正会改动的条数。 */
export interface InboxRulePreview {
  matchCount: number;
  pendingCount: number;
  sampleTitles: string[];
}

/** 后台轮询一轮的结果，作为 `inbox-sync-updated` 事件的载荷；失败时为 `null`。 */
export interface InboxSyncSummary {
  sourceId: string;
  fetchedCount: number;
  ingestedCount: number;
  pendingCount: number;
  syncedAt: string;
}

/** 邮箱连接加密方式：`implicit` 是 993 直连 TLS，`starttls` 是 143 先明文再升级。 */
export type EmailTls = "implicit" | "starttls";

/** 设置页展示的邮箱账户：**不含授权码**，凭据只在 Windows 凭据管理器。 */
export interface EmailAccountStatus {
  id: string;
  address: string;
  host: string;
  port: number;
  tls: EmailTls;
  mailbox: string;
  enabled: boolean;
  lastSyncedAt: string | null;
  lastError: string | null;
}

export interface EmailAccountInput {
  address: string;
  /** 选填：编辑端口 / 加密方式时留空表示沿用已保存的授权码。 */
  password?: string | null;
  host: string;
  /** 留空时后端按加密方式补 993 / 143。 */
  port?: number | null;
  tls?: EmailTls | null;
  mailbox?: string | null;
}

/** 「测试连接」的结果：只回邮箱概况，不回任何凭据。 */
export interface EmailAccountProbe {
  mailboxTotal: number;
  uidValidity: number;
}

/** 开发用入库入参：字段与后端 `IngestInboxMessageInput` 一致。 */
export interface IngestInboxMessageInput {
  externalId: string;
  externalUpdatedAt?: string | null;
  category: string;
  title: string;
  summary?: string | null;
  body?: string | null;
  url?: string | null;
  actor?: string | null;
  occurredAt?: string | null;
  relatedKind?: string | null;
  relatedId?: string | null;
  payloadJson?: string | null;
}

export interface GithubRepo {
  id: string;
  owner: string;
  name: string;
  enabled: boolean;
  lastSyncedAt: string | null;
  etag: string | null;
  description: string | null;
  homepage: string | null;
  language: string | null;
  stars: number | null;
  forks: number | null;
  openIssues: number | null;
  defaultBranch: string | null;
  pushedAt: string | null;
  isArchived: boolean;
  isPrivate: boolean;
  /** README 原文，只在仓库概览里按需展示。 */
  readmeText: string | null;
  readmeSummary: string | null;
  readmeSyncedAt: string | null;
  customDescription: string | null;
  customNote: string | null;
  metaSyncedAt: string | null;
  createdAt: string;
  updatedAt: string;
}

export type ThreadKind = "pull_request" | "issue";
export type ThreadState = "open" | "draft" | "merged" | "closed";
export type ThreadRelationship = "author" | "assigned" | "review_requested" | "reviewed" | "none";

export interface ThreadSummary {
  id: string;
  repoId: string;
  kind: ThreadKind;
  number: number;
  title: string;
  state: ThreadState;
  author: string | null;
  url: string;
  summary: string | null;
  isAuthor: boolean;
  isAssigned: boolean;
  reviewRequested: boolean;
  reviewed: boolean;
  commentCount: number;
  additions: number | null;
  deletions: number | null;
  changedFiles: number | null;
  createdAt: string;
  updatedAt: string;
  closedAt: string | null;
  mergedAt: string | null;
}

export interface ThreadDetail extends ThreadSummary {
  body: string | null;
  /** 本地缓存的评论；打开详情时先渲染它，再按需刷新。 */
  comments: ThreadComment[];
  commentsSyncedAt: string | null;
}

/** 一条对话评论（PR 与 Issue 共用）。 */
export interface ThreadComment {
  id: string;
  threadId: string;
  githubId: number;
  author: string | null;
  body: string;
  url: string;
  createdAt: string;
  updatedAt: string;
  syncedAt: string;
}

/** 刷新结果：拉取失败时退回缓存，`stale` 为 true 并带错误说明。 */
export interface ThreadComments {
  comments: ThreadComment[];
  syncedAt: string | null;
  stale: boolean;
  error: string | null;
}

export interface ThreadPage {
  threads: ThreadSummary[];
  nextCursor: string | null;
  hasMore: boolean;
}

export interface ThreadPageInput {
  repoId: string;
  kind?: ThreadKind | null;
  state?: ThreadState | null;
  relationship?: ThreadRelationship | null;
  cursor?: string | null;
  limit?: number;
}

export interface RepoThreadCounts {
  openPullRequests: number;
  openIssues: number;
  reviews: number;
  releases: number;
  captures: number;
}

export interface RepoSummary {
  id: string;
  owner: string;
  name: string;
  /** 展示用描述：自定义优先。 */
  description: string | null;
  language: string | null;
  stars: number | null;
  openPullRequests: number;
  openIssues: number;
  reviews: number;
  releases: number;
  captures: number;
  enabled: boolean;
  lastSyncedAt: string | null;
  isArchived: boolean;
  isPrivate: boolean;
}

export interface RepoDetail extends GithubRepo {
  counts: RepoThreadCounts;
}

export interface GithubRelease {
  id: string;
  repoId: string;
  tagName: string;
  name: string | null;
  url: string;
  body: string | null;
  summary: string | null;
  isDraft: boolean;
  isPrerelease: boolean;
  publishedAt: string | null;
  createdAt: string;
  syncedAt: string;
}

export interface GithubSyncRun {
  id: string;
  source: string;
  startedAt: string;
  finishedAt: string | null;
  status: "running" | "succeeded" | "failed";
  error: string | null;
}

export interface GithubSyncSummary {
  repoId: string;
  threadsCount: number;
  releasesCount: number;
  /** 新增或内容变化的行数，0 即没有变化。 */
  changedCount: number;
  notModified: boolean;
  syncedAt: string;
}

export interface GithubTokenStatus {
  configured: boolean;
}

export interface ExportResult {
  path: string;
  captureCount: number;
}

const commandErrorCodes: CommandErrorCode[] = ["validation", "not_found", "conflict", "internal"];

function normalizeError(error: unknown): ApiError {
  if (typeof error === "object" && error !== null) {
    const candidate = error as Partial<CommandErrorPayload>;
    if (
      typeof candidate.message === "string" &&
      candidate.code !== undefined &&
      commandErrorCodes.includes(candidate.code)
    ) {
      return new ApiError(candidate.code, candidate.message);
    }
  }

  return new ApiError("internal", typeof error === "string" ? error : "IPC request failed");
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return args ? await invoke<T>(command, args) : await invoke<T>(command);
  } catch (error) {
    throw normalizeError(error);
  }
}

/**
 * 开发模式下的界面走查开关：带 `?preview` 时读接口返回 fixtures。
 * 生产构建里 `import.meta.env.DEV` 为 false，这段连同 fixtures 一起被摇掉。
 */
export const previewMode =
  import.meta.env.DEV && new URLSearchParams(window.location.search).has("preview");

type Fixtures = (typeof import("../features/dev/fixtures"))["fixtures"];

async function preview<T>(pick: (data: Fixtures) => T): Promise<T | null> {
  if (!previewMode) return null;
  const { fixtures } = await import("../features/dev/fixtures");
  return pick(fixtures);
}

export async function getAppInfo(): Promise<AppInfo> {
  const mocked = await preview((data) => data.appInfo);
  return mocked ?? call("get_app_info");
}

export async function getMigrationNotice(): Promise<MigrationNotice> {
  const mocked = await preview((data) => data.migrationNotice);
  return mocked ?? call("get_migration_notice");
}

export function createCapture(input: CreateCaptureInput): Promise<Capture> {
  return call("create_capture", { input });
}

export function captureFromClipboard(input: QuickCaptureInput): Promise<Capture> {
  return call("capture_from_clipboard", { input });
}

export function dismissCapture(): Promise<void> {
  return call<void>("dismiss_capture");
}

/** 保存成功后收起随手记窗口，并通知主窗口刷新列表与角标。 */
export function finishCapture(): Promise<void> {
  return call<void>("finish_capture");
}

export function openCapture(): Promise<void> {
  return call<void>("open_capture");
}

export function getCapture(id: string): Promise<Capture> {
  return call("get_capture", { input: { id } });
}

export async function listCapturesPage(input: CapturePageInput): Promise<CapturePage> {
  const mocked = await preview((data) => data.pageFor(input));
  return mocked ?? call("list_captures_page", { input });
}

export async function countCaptures(kind?: CaptureKind): Promise<CaptureCounts> {
  const mocked = await preview((data) => data.counts);
  return mocked ?? call("count_captures", { input: kind ? { kind } : {} });
}

/** 详情正文按需加载：本地有就返回，链接类为空时会回源抓一次。 */
export async function getCaptureBody(id: string): Promise<CaptureBody> {
  const mocked = await preview((data) => data.bodyFor(id));
  return mocked ?? call("get_capture_body", { input: { id } });
}

export function updateCapture(input: UpdateCaptureInput): Promise<Capture> {
  return call("update_capture", { input });
}

export function archiveCapture(id: string): Promise<Capture> {
  return call("archive_capture", { input: { id } });
}

export function restoreCapture(id: string): Promise<Capture> {
  return call("restore_capture", { input: { id } });
}

export async function searchCaptures(query: string): Promise<CaptureSummary[]> {
  const mocked = await preview((data) => data.search(query));
  return mocked ?? call("search_captures", { input: { query } });
}

export async function listTags(): Promise<Tag[]> {
  const mocked = await preview((data) => data.tags);
  return mocked ?? call("list_tags");
}

export function createTag(input: CreateTagInput): Promise<Tag> {
  return call("create_tag", { input });
}

export function setCaptureTags(captureId: string, tagIds: string[]): Promise<Tag[]> {
  return call("set_capture_tags", { input: { captureId, tagIds } });
}

export async function getCaptureTags(captureId: string): Promise<Tag[]> {
  const mocked = await preview((data) => data.captureTags[captureId] ?? []);
  return mocked ?? call("get_capture_tags", { input: { captureId } });
}

export async function createTask(input: CreateTaskInput): Promise<Task> {
  const mocked = await preview((data) => data.createTask(input));
  return mocked ?? call("create_task", { input });
}

export async function listTasksPage(input: TaskPageInput): Promise<TaskPage> {
  const mocked = await preview((data) => data.tasksPage(input));
  return mocked ?? call("list_tasks_page", { input });
}

export async function countTasks(): Promise<TaskCounts> {
  const mocked = await preview((data) => data.taskCounts());
  return mocked ?? call("count_tasks");
}

export async function updateTask(input: UpdateTaskInput): Promise<Task> {
  const mocked = await preview((data) => data.updateTask(input));
  return mocked ?? call("update_task", { input });
}

export async function completeTask(id: string): Promise<Task> {
  const mocked = await preview((data) => data.setTaskStatus(id, "done"));
  return mocked ?? call("complete_task", { input: { id } });
}

export async function cancelTask(id: string): Promise<Task> {
  const mocked = await preview((data) => data.setTaskStatus(id, "cancelled"));
  return mocked ?? call("cancel_task", { input: { id } });
}

export async function reopenTask(id: string): Promise<Task> {
  const mocked = await preview((data) => data.setTaskStatus(id, "open"));
  return mocked ?? call("reopen_task", { input: { id } });
}

export async function listInboxMessagesPage(input: InboxPageInput): Promise<InboxPage> {
  const mocked = await preview((data) => data.inboxPage(input));
  return mocked ?? call("list_inbox_messages_page", { input });
}

export async function countInboxMessages(input?: InboxCountFilter): Promise<InboxCounts> {
  const mocked = await preview((data) => data.inboxCounts(input));
  return mocked ?? call("count_inbox_messages", { input: input ?? {} });
}

export async function listInboxSources(): Promise<InboxSource[]> {
  const mocked = await preview((data) => data.inboxSources);
  return mocked ?? call("list_inbox_sources");
}

/** 按 id 取一条消息：待办回跳原消息时用，不依赖当前列表的筛选结果。 */
export async function getInboxMessage(id: string): Promise<InboxMessage> {
  const mocked = await preview((data) => data.getInboxMessage(id));
  return mocked ?? call("get_inbox_message", { input: { id } });
}

/**
 * 已读回写队列：`pending` 是还没写出去的意图（失败后按指数退避重试），
 * `blocked` 需要重新授权或手动重试。队列同时承载 GitHub 通知与邮件的已读。
 */
export interface InboxReadBackStatus {
  pending: number;
  blocked: number;
  lastError: string | null;
}

export async function getInboxReadBackStatus(): Promise<InboxReadBackStatus> {
  const mocked = await preview((data) => data.readBackStatus);
  return mocked ?? call("get_inbox_read_back_status");
}

/** 把 blocked 的记录放回队列，返回新的队列概览。 */
export async function retryInboxReadBack(): Promise<InboxReadBackStatus> {
  const mocked = await preview((data) => data.readBackStatus);
  return mocked ?? call("retry_inbox_read_back");
}

/** 收件箱规则：按创建顺序返回，禁用的一起返回。 */
export async function listInboxRules(): Promise<InboxRule[]> {
  const mocked = await preview((data) => data.inboxRules);
  return mocked ?? call("list_inbox_rules");
}

/** 保存规则：空条件会被后端拒绝，避免一条没有条件的规则吞掉整个收件箱。 */
export async function saveInboxRule(input: InboxRuleInput): Promise<InboxRule> {
  const mocked = await preview((data) => data.saveInboxRule(input));
  return mocked ?? call("save_inbox_rule", { input });
}

/** 删除规则后直接返回剩余列表，前端不必再查一次。 */
export async function deleteInboxRule(id: string): Promise<InboxRule[]> {
  const mocked = await preview((data) => data.deleteInboxRule(id));
  return mocked ?? call("delete_inbox_rule", { input: { id } });
}

/** 试跑：只看最近 14 天还有多少条会被命中，不写任何数据。 */
export async function previewInboxRule(input: InboxRuleFilterInput): Promise<InboxRulePreview> {
  const mocked = await preview((data) => data.previewInboxRule(input));
  return mocked ?? call("preview_inbox_rule", { input });
}

export async function markInboxMessageRead(id: string): Promise<InboxMessage> {
  const mocked = await preview((data) => data.markInboxRead(id, true));
  return mocked ?? call("mark_inbox_message_read", { input: { id } });
}

export async function markInboxMessageUnread(id: string): Promise<InboxMessage> {
  const mocked = await preview((data) => data.markInboxRead(id, false));
  return mocked ?? call("mark_inbox_message_unread", { input: { id } });
}

export async function dismissInboxMessage(id: string): Promise<InboxMessage> {
  const mocked = await preview((data) => data.dismissInboxMessage(id));
  return mocked ?? call("dismiss_inbox_message", { input: { id } });
}

/** 转为待办：重复调用返回同一条待办，不会生成第二条。 */
export async function convertInboxMessageToTask(id: string): Promise<Task> {
  const mocked = await preview((data) => data.convertInboxMessage(id));
  return mocked ?? call("convert_inbox_message_to_task", { input: { id } });
}

/** 批量动作：`ids` 与 `scope` 二选一，`scope` 需带 `expectedCount` 对账。 */
export async function applyInboxAction(input: ApplyInboxActionInput): Promise<InboxActionResult> {
  const mocked = await preview((data) => data.applyInboxAction(input));
  return mocked ?? call("apply_inbox_action", { input });
}

/** 撤销上一次动作：把返回的 `appliedIds` / `taskIds` 原样带回来即可。 */
export async function undoInboxAction(input: UndoInboxActionInput): Promise<InboxActionResult> {
  const mocked = await preview((data) => data.undoInboxAction(input));
  return mocked ?? call("undo_inbox_action", { input });
}

/** 设置页的邮箱账户列表；已删除的账户会从列表消失，但历史消息仍在收件箱。 */
export async function listEmailAccounts(): Promise<EmailAccountStatus[]> {
  const mocked = await preview((data) => data.emailAccounts);
  return mocked ?? call("list_email_accounts");
}

/** 只连接验证，不落库；新增账户时授权码必填。 */
export async function testEmailAccount(input: EmailAccountInput): Promise<EmailAccountProbe> {
  const mocked = await preview((data) => data.probeEmailAccount(input));
  return mocked ?? call("test_email_account", { input });
}

/** 保存会先试连再落库，成功后后台立即同步一次该账户。 */
export async function saveEmailAccount(input: EmailAccountInput): Promise<EmailAccountStatus> {
  const mocked = await preview((data) => data.saveEmailAccount(input));
  return mocked ?? call("save_email_account", { input });
}

/** 删除账户只做软停用：凭据与配置清掉，历史邮件留在收件箱。 */
export async function deleteEmailAccount(id: string): Promise<EmailAccountStatus[]> {
  const mocked = await preview((data) => data.deleteEmailAccount(id));
  return mocked ?? call("delete_email_account", { input: { id } });
}

/** 只用于开发窗口造消息；正式构建里后端会直接拒绝。 */
export function ingestInboxMessage(input: IngestInboxMessageInput): Promise<InboxMessage> {
  return call("ingest_inbox_message", { input });
}

export function addGithubRepo(url: string): Promise<GithubRepo> {
  return call("add_github_repo", { input: { url } });
}

export async function listGithubRepos(): Promise<GithubRepo[]> {
  const mocked = await preview((data) => data.githubRepos);
  return mocked ?? call("list_github_repos");
}

export function removeGithubRepo(id: string): Promise<void> {
  return call("remove_github_repo", { input: { id } });
}

export async function getGithubTokenStatus(): Promise<GithubTokenStatus> {
  const mocked = await preview(() => ({ configured: true }));
  return mocked ?? call("get_github_token_status");
}

export function saveGithubToken(token: string): Promise<GithubTokenStatus> {
  return call("save_github_token", { input: { token } });
}

export function deleteGithubToken(): Promise<GithubTokenStatus> {
  return call("delete_github_token");
}

export type GithubSyncScope = "incremental" | "full";

/** `full` 是显式的「加载历史」：拉全部状态并忽略增量游标。 */
export function syncGithubRepo(
  id: string,
  scope: GithubSyncScope = "incremental",
): Promise<GithubSyncSummary> {
  return call("sync_github_repo", { input: { id, scope } });
}

export function syncAllGithubRepos(): Promise<GithubSyncSummary[]> {
  return call("sync_all_github_repos");
}

export async function listGithubSyncRuns(): Promise<GithubSyncRun[]> {
  const mocked = await preview((data) => data.syncRuns);
  return mocked ?? call("list_github_sync_runs");
}

export async function listRepos(): Promise<RepoSummary[]> {
  const mocked = await preview((data) => data.repos);
  return mocked ?? call("list_repos");
}

export async function getRepo(id: string): Promise<RepoDetail> {
  const mocked = await preview((data) => data.repo);
  return mocked ?? call("get_repo", { input: { id } });
}

/** 自定义描述与备注；传空字符串表示清除。 */
export function updateRepoMeta(input: {
  id: string;
  customDescription: string | null;
  customNote: string | null;
}): Promise<RepoDetail> {
  return call("update_repo_meta", { input });
}

export async function listRepoThreads(input: ThreadPageInput): Promise<ThreadPage> {
  const mocked = await preview((data) => data.threadsFor(input));
  return mocked ?? call("list_repo_threads", { input });
}

export async function getThread(id: string): Promise<ThreadDetail> {
  const mocked = await preview((data) => data.detailFor(id));
  return mocked ?? call("get_thread", { input: { id } });
}

/** 评论按需拉取：本地够新时后端直接返回缓存，`force` 用于手动刷新。 */
export async function refreshThreadComments(id: string, force = false): Promise<ThreadComments> {
  const mocked = await preview((data) => data.commentsFor(id));
  return mocked ?? call("refresh_thread_comments", { input: { id, force } });
}

export async function listRepoReleases(repoId: string): Promise<GithubRelease[]> {
  const mocked = await preview((data) => data.releases);
  return mocked ?? call("list_repo_releases", { input: { id: repoId } });
}

export function exportCapturesJson(): Promise<ExportResult | null> {
  return call("export_captures_json");
}

export function exportCapturesMarkdown(): Promise<ExportResult | null> {
  return call("export_captures_markdown");
}
