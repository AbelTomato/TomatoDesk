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
const previewEnabled =
  import.meta.env.DEV && new URLSearchParams(window.location.search).has("preview");

type Fixtures = (typeof import("../features/dev/fixtures"))["fixtures"];

async function preview<T>(pick: (data: Fixtures) => T): Promise<T | null> {
  if (!previewEnabled) return null;
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
