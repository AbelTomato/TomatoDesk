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

export type ItemKind = "note" | "link" | "github";
export type ItemStatus = "pending" | "processed" | "archived";

export interface Item {
  id: string;
  kind: ItemKind;
  source: string;
  url: string | null;
  title: string | null;
  summary: string | null;
  note: string | null;
  status: ItemStatus;
  createdAt: string;
  updatedAt: string;
}

export interface CreateItemInput {
  kind: ItemKind;
  source: string;
  url: string | null;
  title: string | null;
  summary: string | null;
  note: string | null;
}

export interface CaptureItemInput {
  value: string;
  note: string | null;
}

export interface UpdateItemInput extends CreateItemInput {
  id: string;
  status: ItemStatus;
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
  createdAt: string;
  updatedAt: string;
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
  importedCount: number;
  releasesCount: number;
  issuesCount: number;
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

export function listItems(): Promise<Item[]> {
  return call("list_items");
}

export function updateItem(input: UpdateItemInput): Promise<Item> {
  return call("update_item", { input });
}

export function archiveItem(id: string): Promise<Item> {
  return call("archive_item", { input: { id } });
}

export function searchItems(query: string): Promise<Item[]> {
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

export function syncGithubRepo(id: string): Promise<GithubSyncSummary> {
  return call("sync_github_repo", { input: { id } });
}

export function syncAllGithubRepos(): Promise<GithubSyncSummary[]> {
  return call("sync_all_github_repos");
}

export function listGithubSyncRuns(): Promise<GithubSyncRun[]> {
  return call("list_github_sync_runs");
}

export function exportItemsJson(): Promise<ExportResult | null> {
  return call("export_items_json");
}

export function exportItemsMarkdown(): Promise<ExportResult | null> {
  return call("export_items_markdown");
}
