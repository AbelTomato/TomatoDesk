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
}

export type ItemKind = "note" | "link" | "github";
export type ItemStatus = "pending" | "processed" | "archived";
export type ItemSort = "updated" | "created";

/** 列表行：不含正文，首屏只传索引字段。 */
export interface ItemSummary {
  id: string;
  kind: ItemKind;
  source: string;
  url: string | null;
  title: string | null;
  summary: string | null;
  note: string | null;
  /** 挂在某个仓库下时指向该仓库。 */
  repoId: string | null;
  status: ItemStatus;
  createdAt: string;
  updatedAt: string;
}

export interface Item extends ItemSummary {
  body: string | null;
}

export interface ItemPage {
  items: ItemSummary[];
  nextCursor: string | null;
  hasMore: boolean;
}

export interface ItemPageInput {
  status?: ItemStatus | null;
  kind?: ItemKind | null;
  repoId?: string | null;
  sort?: ItemSort;
  cursor?: string | null;
  limit?: number;
}

export interface ItemStatusCounts {
  all: number;
  pending: number;
  processed: number;
  archived: number;
}

export interface ItemBody {
  id: string;
  kind: ItemKind;
  url: string | null;
  body: string | null;
}

export interface CreateItemInput {
  kind: ItemKind;
  source: string;
  url: string | null;
  title: string | null;
  summary: string | null;
  note: string | null;
  repoId: string | null;
}

export interface CaptureItemInput {
  value: string;
  note: string | null;
}

/** 更新只改内容与状态；条目与仓库的关联在创建时确定，不随编辑变化。 */
export interface UpdateItemInput {
  id: string;
  kind: ItemKind;
  source: string;
  url: string | null;
  title: string | null;
  summary: string | null;
  note: string | null;
  status: ItemStatus;
}

/**
 * 列表行转成更新入参：后端拒绝未知字段，所以只挑 DTO 里的字段，
 * 不能直接展开 `ItemSummary`（会带上 createdAt / updatedAt / repoId）。
 */
export function toUpdateItemInput(item: ItemSummary, status: ItemStatus): UpdateItemInput {
  return {
    id: item.id,
    kind: item.kind,
    source: item.source,
    url: item.url,
    title: item.title,
    summary: item.summary,
    note: item.note,
    status,
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
  items: number;
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
  items: number;
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
  itemCount: number;
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

export function getAppInfo(): Promise<AppInfo> {
  return call("get_app_info");
}

export function getMigrationNotice(): Promise<MigrationNotice> {
  return call("get_migration_notice");
}

export function createItem(input: CreateItemInput): Promise<Item> {
  return call("create_item", { input });
}

export function captureItem(input: CaptureItemInput): Promise<Item> {
  return call("capture_item", { input });
}

export function dismissCapture(): Promise<void> {
  return call<void>("dismiss_capture");
}

export function openCapture(): Promise<void> {
  return call<void>("open_capture");
}

export function getItem(id: string): Promise<Item> {
  return call("get_item", { input: { id } });
}

export function listItemsPage(input: ItemPageInput): Promise<ItemPage> {
  return call("list_items_page", { input });
}

export function countItemsByStatus(kind?: ItemKind): Promise<ItemStatusCounts> {
  return call("count_items_by_status", { input: kind ? { kind } : {} });
}

/** 详情正文按需加载：本地有就返回，链接类为空时会回源抓一次。 */
export function getItemBody(id: string): Promise<ItemBody> {
  return call("get_item_body", { input: { id } });
}

export function updateItem(input: UpdateItemInput): Promise<Item> {
  return call("update_item", { input });
}

export function archiveItem(id: string): Promise<Item> {
  return call("archive_item", { input: { id } });
}

export function searchItems(query: string): Promise<ItemSummary[]> {
  return call("search_items", { input: { query } });
}

export function listTags(): Promise<Tag[]> {
  return call("list_tags");
}

export function createTag(input: CreateTagInput): Promise<Tag> {
  return call("create_tag", { input });
}

export function setItemTags(itemId: string, tagIds: string[]): Promise<Tag[]> {
  return call("set_item_tags", { input: { itemId, tagIds } });
}

export function getItemTags(itemId: string): Promise<Tag[]> {
  return call("get_item_tags", { input: { itemId } });
}

export function addGithubRepo(url: string): Promise<GithubRepo> {
  return call("add_github_repo", { input: { url } });
}

export function listGithubRepos(): Promise<GithubRepo[]> {
  return call("list_github_repos");
}

export function removeGithubRepo(id: string): Promise<void> {
  return call("remove_github_repo", { input: { id } });
}

export function getGithubTokenStatus(): Promise<GithubTokenStatus> {
  return call("get_github_token_status");
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

export function listGithubSyncRuns(): Promise<GithubSyncRun[]> {
  return call("list_github_sync_runs");
}

export function listRepos(): Promise<RepoSummary[]> {
  return call("list_repos");
}

export function getRepo(id: string): Promise<RepoDetail> {
  return call("get_repo", { input: { id } });
}

/** 自定义描述与备注；传空字符串表示清除。 */
export function updateRepoMeta(input: {
  id: string;
  customDescription: string | null;
  customNote: string | null;
}): Promise<RepoDetail> {
  return call("update_repo_meta", { input });
}

export function listRepoThreads(input: ThreadPageInput): Promise<ThreadPage> {
  return call("list_repo_threads", { input });
}

export function getThread(id: string): Promise<ThreadDetail> {
  return call("get_thread", { input: { id } });
}

export function listRepoReleases(repoId: string): Promise<GithubRelease[]> {
  return call("list_repo_releases", { input: { id: repoId } });
}

export function exportItemsJson(): Promise<ExportResult | null> {
  return call("export_items_json");
}

export function exportItemsMarkdown(): Promise<ExportResult | null> {
  return call("export_items_markdown");
}
