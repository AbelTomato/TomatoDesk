import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openUrl } from "@tauri-apps/plugin-opener";

import { Tabs } from "../../components/ui";
import {
  createItem,
  getRepo,
  getThread,
  listItemsPage,
  listRepoReleases,
  listRepoThreads,
  syncGithubRepo,
  toUpdateItemInput,
  updateItem,
  updateRepoMeta,
  type GithubRelease,
  type GithubSyncScope,
  type ItemStatus,
  type ItemSummary,
  type RepoDetail as RepoDetailData,
  type ThreadDetail,
  type ThreadKind,
  type ThreadRelationship,
  type ThreadState,
  type ThreadSummary,
} from "../../lib/api";
import { notifyItemsChanged } from "../../lib/events";
import { formatCount, formatDate, formatRelative } from "../../lib/format";

type TabId = "overview" | "pulls" | "issues" | "reviews" | "releases" | "items";

const PAGE_SIZE = 50;

const stateLabels: Record<ThreadState, string> = {
  open: "打开",
  draft: "草稿",
  merged: "已合并",
  closed: "已关闭",
};

const itemStatusLabels: Record<ItemStatus, string> = {
  pending: "待处理",
  processed: "已处理",
  archived: "已归档",
};

function relationLabel(thread: ThreadSummary): string | null {
  if (thread.isAuthor) return "我发起";
  if (thread.reviewRequested) return "待我评审";
  if (thread.reviewed) return "我评审过";
  if (thread.isAssigned) return "指派给我";
  return null;
}

async function openExternal(url: string) {
  if (isTauri()) await openUrl(url);
  else window.open(url, "_blank", "noopener,noreferrer");
}

/** 失焦保存的行内编辑；空值表示清除。 */
function InlineEdit({
  value,
  placeholder,
  multiline = false,
  onSave,
}: {
  value: string | null;
  placeholder: string;
  multiline?: boolean;
  onSave: (next: string) => Promise<void>;
}) {
  const [draft, setDraft] = useState(value ?? "");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    setDraft(value ?? "");
  }, [value]);

  async function commit() {
    const next = draft.trim();
    if (next === (value ?? "") || saving) return;
    setSaving(true);
    try {
      await onSave(next);
    } finally {
      setSaving(false);
    }
  }

  const className =
    "w-full rounded-lg border border-input bg-transparent px-2.5 py-1.5 text-sm leading-6 outline-none transition placeholder:text-muted-foreground focus-visible:ring-3 focus-visible:ring-ring/50 disabled:opacity-50";

  return multiline ? (
    <textarea
      className={`${className} min-h-16 resize-y`}
      value={draft}
      placeholder={placeholder}
      disabled={saving}
      onChange={(event) => setDraft(event.target.value)}
      onBlur={() => void commit()}
    />
  ) : (
    <input
      className={className}
      value={draft}
      placeholder={placeholder}
      disabled={saving}
      onChange={(event) => setDraft(event.target.value)}
      onBlur={() => void commit()}
    />
  );
}

function OverviewTab({
  repo,
  onSaveMeta,
}: {
  repo: RepoDetailData;
  onSaveMeta: (description: string, note: string) => Promise<void>;
}) {
  const [showReadme, setShowReadme] = useState(false);
  const homepage = repo.homepage;

  return (
    <div className="grid gap-6 px-5 py-6 sm:px-7 lg:grid-cols-[minmax(0,1.6fr)_minmax(16rem,1fr)]">
      <div className="space-y-6">
        <section>
          <h3 className="font-mono text-[10px] tracking-wider text-muted-foreground uppercase">
            描述
          </h3>
          <p className="mt-2 text-sm leading-7 whitespace-pre-wrap text-foreground/85">
            {repo.description ?? "仓库没有填写描述。"}
          </p>
        </section>

        <section>
          <div className="flex items-center justify-between gap-3">
            <h3 className="font-mono text-[10px] tracking-wider text-muted-foreground uppercase">
              README 摘要
            </h3>
            {repo.readmeText ? (
              <button
                className="text-xs font-medium text-link hover:underline"
                type="button"
                onClick={() => setShowReadme((current) => !current)}
              >
                {showReadme ? "收起原文" : "查看原文"}
              </button>
            ) : null}
          </div>
          {repo.readmeSummary ? (
            <p className="mt-2 text-sm leading-7 text-foreground/85">{repo.readmeSummary}</p>
          ) : (
            <p className="mt-2 text-sm text-muted-foreground">
              还没有 README 摘要，同步后会从 README 生成纯文本摘要。
            </p>
          )}
          {showReadme && repo.readmeText ? (
            <pre className="mt-3 max-h-96 overflow-auto rounded-lg border border-border bg-background p-3 font-mono text-xs leading-6 whitespace-pre-wrap text-foreground/80">
              {repo.readmeText}
            </pre>
          ) : null}
        </section>

        <section>
          <h3 className="font-mono text-[10px] tracking-wider text-muted-foreground uppercase">
            自定义描述
          </h3>
          <p className="mt-1 text-xs text-muted-foreground">
            失焦保存，留空则回到仓库自带的 description。
          </p>
          <div className="mt-2">
            <InlineEdit
              value={repo.customDescription}
              placeholder="给自己写一句这个仓库的定位"
              onSave={(next) => onSaveMeta(next, repo.customNote ?? "")}
            />
          </div>
        </section>

        <section>
          <h3 className="font-mono text-[10px] tracking-wider text-muted-foreground uppercase">
            备注
          </h3>
          <div className="mt-2">
            <InlineEdit
              value={repo.customNote}
              placeholder="记录待办、分支约定、发布节奏…"
              multiline
              onSave={(next) => onSaveMeta(repo.customDescription ?? "", next)}
            />
          </div>
        </section>
      </div>

      <aside className="space-y-4">
        <div className="rounded-xl border border-border bg-background p-4">
          <h3 className="font-mono text-[10px] tracking-wider text-muted-foreground uppercase">
            仓库信息
          </h3>
          <dl className="mt-3 space-y-2 text-sm">
            {[
              ["语言", repo.language ?? "—"],
              ["Stars", formatCount(repo.stars)],
              ["Forks", formatCount(repo.forks)],
              ["未关闭 Issue", formatCount(repo.openIssues)],
              ["默认分支", repo.defaultBranch ?? "—"],
              ["最近推送", formatRelative(repo.pushedAt)],
              ["最近同步", formatRelative(repo.metaSyncedAt)],
            ].map(([label, value]) => (
              <div key={label} className="flex items-baseline justify-between gap-3">
                <dt className="text-muted-foreground">{label}</dt>
                <dd className="font-mono text-xs text-foreground/85">{value}</dd>
              </div>
            ))}
          </dl>
        </div>

        <div className="rounded-xl border border-border bg-background p-4">
          <h3 className="font-mono text-[10px] tracking-wider text-muted-foreground uppercase">
            当前待办
          </h3>
          <dl className="mt-3 space-y-2 text-sm">
            {[
              ["打开的 PR", repo.counts.openPullRequests],
              ["打开的 Issue", repo.counts.openIssues],
              ["参与评审", repo.counts.reviews],
              ["Release", repo.counts.releases],
              ["我的条目", repo.counts.items],
            ].map(([label, value]) => (
              <div key={label} className="flex items-baseline justify-between gap-3">
                <dt className="text-muted-foreground">{label}</dt>
                <dd className="font-mono text-xs text-foreground/85">{value}</dd>
              </div>
            ))}
          </dl>
        </div>

        {homepage ? (
          <button
            className="w-full truncate rounded-lg border border-border px-3 py-2 text-left text-xs text-link hover:bg-muted"
            type="button"
            onClick={() => void openExternal(homepage)}
          >
            {homepage}
          </button>
        ) : null}
      </aside>
    </div>
  );
}

function ThreadsTab({
  repoId,
  kind,
  relationship,
}: {
  repoId: string;
  kind: ThreadKind;
  relationship?: ThreadRelationship;
}) {
  const [stateFilter, setStateFilter] = useState<"open" | "all">("open");
  const [threads, setThreads] = useState<ThreadSummary[]>([]);
  const [cursor, setCursor] = useState<string | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<ThreadDetail | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const requestId = useRef(0);

  const loadFirstPage = useCallback(async () => {
    const current = ++requestId.current;
    setLoading(true);
    setError(null);
    try {
      const page = await listRepoThreads({
        repoId,
        kind,
        relationship: relationship ?? null,
        state: stateFilter === "open" ? "open" : null,
        limit: PAGE_SIZE,
      });
      if (current !== requestId.current) return;
      setThreads(page.threads);
      setCursor(page.nextCursor);
      setHasMore(page.hasMore);
    } catch (loadError) {
      if (current !== requestId.current) return;
      setThreads([]);
      setCursor(null);
      setHasMore(false);
      setError(loadError instanceof Error ? loadError.message : "加载列表失败，请重试。");
    } finally {
      if (current === requestId.current) setLoading(false);
    }
  }, [kind, relationship, repoId, stateFilter]);

  useEffect(() => {
    void loadFirstPage();
  }, [loadFirstPage]);

  useEffect(() => {
    if (threads.length === 0) {
      setSelectedId(null);
      return;
    }
    if (!threads.some((thread) => thread.id === selectedId)) setSelectedId(threads[0].id);
  }, [selectedId, threads]);

  useEffect(() => {
    if (!selectedId) {
      setDetail(null);
      return;
    }
    let disposed = false;
    setDetailLoading(true);
    void getThread(selectedId)
      .then((value) => {
        if (!disposed) setDetail(value);
      })
      .catch(() => {
        if (!disposed) setDetail(null);
      })
      .finally(() => {
        if (!disposed) setDetailLoading(false);
      });
    return () => {
      disposed = true;
    };
  }, [selectedId]);

  async function loadMore() {
    if (!hasMore || loadingMore || !cursor) return;
    const current = requestId.current;
    setLoadingMore(true);
    try {
      const page = await listRepoThreads({
        repoId,
        kind,
        relationship: relationship ?? null,
        state: stateFilter === "open" ? "open" : null,
        cursor,
        limit: PAGE_SIZE,
      });
      if (current !== requestId.current) return;
      setThreads((existing) => {
        const seen = new Set(existing.map((thread) => thread.id));
        return [...existing, ...page.threads.filter((thread) => !seen.has(thread.id))];
      });
      setCursor(page.nextCursor);
      setHasMore(page.hasMore);
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : "加载更多失败，请重试。");
    } finally {
      setLoadingMore(false);
    }
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-2 border-b border-border px-5 py-3 sm:px-7">
        {(["open", "all"] as const).map((value) => (
          <button
            key={value}
            className={`h-7 rounded-md px-2.5 font-mono text-xs transition ${stateFilter === value ? "bg-primary text-primary-foreground" : "text-muted-foreground hover:bg-accent hover:text-foreground"}`}
            type="button"
            onClick={() => setStateFilter(value)}
          >
            {value === "open" ? "打开中" : "全部"}
          </button>
        ))}
        <span className="ml-auto font-mono text-[11px] text-muted-foreground">
          共 {threads.length} 条{hasMore ? "（还有更多）" : ""}
        </span>
      </div>

      <div className="grid min-h-0 flex-1 lg:grid-cols-[minmax(20rem,1.1fr)_minmax(20rem,0.9fr)]">
        <div className="border-b border-border p-4 sm:p-5 lg:border-r lg:border-b-0">
          {loading ? (
            <div className="space-y-2" role="status" aria-label="正在加载">
              {Array.from({ length: 4 }).map((_, index) => (
                <div
                  key={index}
                  className="rounded-lg border border-border bg-card p-3"
                  aria-hidden
                >
                  <div className="h-3.5 w-3/4 animate-pulse rounded bg-muted" />
                  <div className="mt-2 h-3 w-1/3 animate-pulse rounded bg-muted/70" />
                </div>
              ))}
            </div>
          ) : null}
          {!loading && error ? (
            <div className="rounded-lg border border-destructive/30 bg-destructive/10 p-4 text-sm text-destructive">
              <p>{error}</p>
              <button
                className="mt-2 font-medium underline"
                type="button"
                onClick={() => void loadFirstPage()}
              >
                重新加载
              </button>
            </div>
          ) : null}
          {!loading && !error && threads.length === 0 ? (
            <div className="rounded-lg border border-dashed border-border bg-card p-8 text-center text-sm text-muted-foreground">
              没有符合条件的内容，同步一次或切换筛选试试。
            </div>
          ) : null}

          <div className="space-y-2">
            {threads.map((thread) => {
              const active = thread.id === selectedId;
              const relation = relationLabel(thread);
              return (
                <button
                  key={thread.id}
                  className={`block w-full rounded-lg border p-3 text-left transition ${active ? "border-link/60 bg-card" : "border-border bg-card hover:border-foreground/30"}`}
                  type="button"
                  aria-current={active ? "true" : undefined}
                  onClick={() => setSelectedId(thread.id)}
                >
                  <div className="flex items-start justify-between gap-3">
                    <span className="line-clamp-2 min-w-0 text-sm leading-5 font-medium">
                      {thread.title}
                    </span>
                    <span className="shrink-0 rounded bg-muted px-1.5 py-0.5 font-mono text-[10px] tracking-wider text-muted-foreground uppercase">
                      {stateLabels[thread.state]}
                    </span>
                  </div>
                  <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 font-mono text-[11px] text-muted-foreground">
                    <span>#{thread.number}</span>
                    {relation ? <span className="text-link">{relation}</span> : null}
                    {thread.kind === "pull_request" && thread.changedFiles !== null ? (
                      <span>
                        +{thread.additions ?? 0} −{thread.deletions ?? 0}
                      </span>
                    ) : null}
                    <span>💬 {thread.commentCount}</span>
                    <span className="ml-auto">{formatRelative(thread.updatedAt)}</span>
                  </div>
                </button>
              );
            })}
          </div>

          {!loading && !error ? (
            <div className="mt-3 flex justify-center">
              {loadingMore ? (
                <span className="font-mono text-[11px] text-muted-foreground">正在加载更多…</span>
              ) : hasMore ? (
                <button
                  className="h-8 rounded-lg border border-border px-3 text-xs text-foreground/80 transition hover:bg-muted"
                  type="button"
                  onClick={() => void loadMore()}
                >
                  加载更多
                </button>
              ) : threads.length > 0 ? (
                <span className="font-mono text-[11px] text-muted-foreground">已显示全部</span>
              ) : null}
            </div>
          ) : null}
        </div>

        <div className="min-w-0 bg-card p-5 sm:p-6">
          {detail ? (
            <div className="mx-auto max-w-2xl">
              <div className="flex flex-wrap items-center gap-2">
                <span className="rounded bg-muted px-1.5 py-0.5 font-mono text-[10px] tracking-wider text-muted-foreground uppercase">
                  {detail.kind === "pull_request" ? "PR" : "Issue"} #{detail.number}
                </span>
                <span className="rounded bg-muted px-1.5 py-0.5 font-mono text-[10px] tracking-wider text-muted-foreground uppercase">
                  {stateLabels[detail.state]}
                </span>
                {relationLabel(detail) ? (
                  <span className="font-mono text-[11px] text-link">{relationLabel(detail)}</span>
                ) : null}
              </div>
              <h3 className="mt-3 text-xl leading-snug font-bold tracking-tight break-words">
                {detail.title}
              </h3>
              <p className="mt-2 font-mono text-xs text-muted-foreground">
                {detail.author ?? "未知作者"} · 更新于 {formatDate(detail.updatedAt)}
                {detail.mergedAt ? ` · 合并于 ${formatDate(detail.mergedAt)}` : ""}
              </p>
              <button
                className="mt-4 h-8 rounded-lg border border-border px-3 text-sm text-foreground/80 transition hover:bg-muted"
                type="button"
                onClick={() => void openExternal(detail.url)}
              >
                在 GitHub 打开
              </button>

              <div className="mt-6">
                <p className="font-mono text-[10px] tracking-wider text-muted-foreground uppercase">
                  正文
                </p>
                {detailLoading ? (
                  <div className="mt-2 space-y-2" role="status" aria-label="正在加载正文">
                    {Array.from({ length: 3 }).map((_, index) => (
                      <div
                        key={index}
                        className="h-3 animate-pulse rounded bg-muted"
                        style={{ width: `${90 - index * 15}%` }}
                      />
                    ))}
                  </div>
                ) : detail.body?.trim() ? (
                  <p className="mt-2 text-sm leading-7 whitespace-pre-wrap text-foreground/80">
                    {detail.body}
                  </p>
                ) : (
                  <p className="mt-2 text-sm text-muted-foreground">这条内容没有正文。</p>
                )}
              </div>
            </div>
          ) : (
            <div className="flex min-h-48 items-center justify-center text-center text-sm text-muted-foreground">
              选择一条内容查看正文
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function ReleasesTab({ repoId }: { repoId: string }) {
  const [releases, setReleases] = useState<GithubRelease[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);

  useEffect(() => {
    let disposed = false;
    setLoading(true);
    void listRepoReleases(repoId)
      .then((value) => {
        if (!disposed) setReleases(value);
      })
      .catch((loadError) => {
        if (!disposed) {
          setError(loadError instanceof Error ? loadError.message : "加载 Release 失败。");
        }
      })
      .finally(() => {
        if (!disposed) setLoading(false);
      });
    return () => {
      disposed = true;
    };
  }, [repoId]);

  if (loading) {
    return <p className="px-5 py-6 font-mono text-xs text-muted-foreground sm:px-7">正在加载…</p>;
  }
  if (error) {
    return <p className="px-5 py-6 text-sm text-destructive sm:px-7">{error}</p>;
  }
  if (releases.length === 0) {
    return (
      <p className="px-5 py-6 text-sm text-muted-foreground sm:px-7">
        这个仓库还没有 Release，同步后会出现在这里。
      </p>
    );
  }

  return (
    <div className="px-5 py-5 sm:px-7">
      <div className="overflow-hidden rounded-xl border border-border bg-card">
        {releases.map((release) => (
          <div key={release.id} className="border-b border-border px-4 py-4 last:border-b-0">
            <div className="flex flex-wrap items-center gap-2">
              <span className="font-mono text-sm text-link">{release.tagName}</span>
              {release.name ? <span className="text-sm font-medium">{release.name}</span> : null}
              {release.isPrerelease ? (
                <span className="rounded bg-muted px-1.5 py-0.5 font-mono text-[10px] tracking-wider text-muted-foreground uppercase">
                  预发布
                </span>
              ) : null}
              {release.isDraft ? (
                <span className="rounded bg-muted px-1.5 py-0.5 font-mono text-[10px] tracking-wider text-muted-foreground uppercase">
                  草稿
                </span>
              ) : null}
              <span className="ml-auto font-mono text-[11px] text-muted-foreground">
                {formatDate(release.publishedAt ?? release.createdAt)}
              </span>
            </div>
            {release.body ? (
              <>
                <p
                  className={`mt-2 text-sm leading-6 whitespace-pre-wrap text-foreground/80 ${
                    expanded === release.id ? "" : "line-clamp-3"
                  }`}
                >
                  {release.body}
                </p>
                <div className="mt-2 flex gap-3">
                  <button
                    className="text-xs font-medium text-link hover:underline"
                    type="button"
                    onClick={() =>
                      setExpanded((current) => (current === release.id ? null : release.id))
                    }
                  >
                    {expanded === release.id ? "收起" : "展开全文"}
                  </button>
                  <button
                    className="text-xs text-muted-foreground hover:underline"
                    type="button"
                    onClick={() => void openExternal(release.url)}
                  >
                    在 GitHub 打开
                  </button>
                </div>
              </>
            ) : null}
          </div>
        ))}
      </div>
    </div>
  );
}

function RepoItemsTab({ repoId }: { repoId: string }) {
  const [items, setItems] = useState<ItemSummary[]>([]);
  const [cursor, setCursor] = useState<string | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const page = await listItemsPage({ repoId, limit: PAGE_SIZE });
      setItems(page.items);
      setCursor(page.nextCursor);
      setHasMore(page.hasMore);
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : "加载条目失败。");
    } finally {
      setLoading(false);
    }
  }, [repoId]);

  useEffect(() => {
    void load();
  }, [load]);

  async function loadMore() {
    if (!hasMore || !cursor || busy) return;
    setBusy(true);
    try {
      const page = await listItemsPage({ repoId, cursor, limit: PAGE_SIZE });
      setItems((current) => {
        const seen = new Set(current.map((item) => item.id));
        return [...current, ...page.items.filter((item) => !seen.has(item.id))];
      });
      setCursor(page.nextCursor);
      setHasMore(page.hasMore);
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : "加载更多失败。");
    } finally {
      setBusy(false);
    }
  }

  async function addNote(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const note = draft.trim();
    if (!note || busy) return;
    setBusy(true);
    setError(null);
    try {
      await createItem({
        kind: "note",
        source: "repo",
        url: null,
        title: null,
        summary: null,
        note,
        repoId,
      });
      setDraft("");
      await load();
      notifyItemsChanged();
    } catch (createError) {
      setError(createError instanceof Error ? createError.message : "添加条目失败。");
    } finally {
      setBusy(false);
    }
  }

  async function changeStatus(item: ItemSummary, status: ItemStatus) {
    if (item.status === status || busy) return;
    setBusy(true);
    try {
      const updated = await updateItem(toUpdateItemInput(item, status));
      setItems((current) =>
        current.map((entry) => (entry.id === updated.id ? { ...entry, ...updated } : entry)),
      );
      notifyItemsChanged();
    } catch (updateError) {
      setError(updateError instanceof Error ? updateError.message : "更新状态失败。");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="px-5 py-5 sm:px-7">
      <form className="flex flex-col gap-2 sm:flex-row" onSubmit={addNote}>
        <textarea
          className="min-h-16 min-w-0 flex-1 resize-y rounded-lg border border-input bg-transparent px-2.5 py-1.5 text-sm leading-6 outline-none placeholder:text-muted-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
          value={draft}
          placeholder="记一条与这个仓库相关的笔记…"
          maxLength={16_384}
          onChange={(event) => setDraft(event.target.value)}
        />
        <button
          className="h-8 shrink-0 self-start rounded-lg bg-primary px-3 text-sm font-medium text-primary-foreground transition hover:bg-primary/90 disabled:opacity-50"
          type="submit"
          disabled={busy || !draft.trim()}
        >
          添加条目
        </button>
      </form>

      {error ? <p className="mt-3 text-sm text-destructive">{error}</p> : null}

      <div className="mt-4 overflow-hidden rounded-xl border border-border bg-card">
        {loading ? (
          <p className="px-4 py-6 font-mono text-xs text-muted-foreground">正在加载…</p>
        ) : items.length === 0 ? (
          <p className="px-4 py-6 text-sm text-muted-foreground">
            还没有挂在这个仓库下的条目，可以在上面记一条。
          </p>
        ) : (
          items.map((item) => (
            <div key={item.id} className="border-b border-border px-4 py-3 last:border-b-0">
              <p className="text-sm leading-6 whitespace-pre-wrap text-foreground/85">
                {item.title ?? item.note ?? item.summary ?? item.source}
              </p>
              <div className="mt-2 flex flex-wrap items-center gap-2">
                <span className="rounded bg-muted px-1.5 py-0.5 font-mono text-[10px] tracking-wider text-muted-foreground uppercase">
                  {itemStatusLabels[item.status]}
                </span>
                <time className="font-mono text-[11px] text-muted-foreground">
                  {formatRelative(item.updatedAt)}
                </time>
                <div className="ml-auto flex gap-1">
                  {(["pending", "processed", "archived"] as const).map((status) => (
                    <button
                      key={status}
                      className={`h-7 rounded-md px-2 font-mono text-[11px] transition ${item.status === status ? "bg-primary text-primary-foreground" : "text-muted-foreground hover:bg-accent hover:text-foreground"}`}
                      type="button"
                      disabled={busy}
                      onClick={() => void changeStatus(item, status)}
                    >
                      {itemStatusLabels[status]}
                    </button>
                  ))}
                </div>
              </div>
            </div>
          ))
        )}
      </div>

      {hasMore ? (
        <div className="mt-3 flex justify-center">
          <button
            className="h-8 rounded-lg border border-border px-3 text-xs text-foreground/80 transition hover:bg-muted disabled:opacity-50"
            type="button"
            disabled={busy}
            onClick={() => void loadMore()}
          >
            加载更多
          </button>
        </div>
      ) : null}
    </div>
  );
}

function RepoDetail({ repoId, onBack }: { repoId: string; onBack: () => void }) {
  const [repo, setRepo] = useState<RepoDetailData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [syncing, setSyncing] = useState(false);
  const [tab, setTab] = useState<TabId>("overview");
  const [reviewScope, setReviewScope] = useState<ThreadRelationship>("review_requested");

  const load = useCallback(async () => {
    setError(null);
    try {
      setRepo(await getRepo(repoId));
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : "加载仓库失败，请重试。");
    } finally {
      setLoading(false);
    }
  }, [repoId]);

  useEffect(() => {
    setLoading(true);
    setTab("overview");
    void load();
  }, [load]);

  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    const listener = getCurrentWindow().listen("github-sync-updated", () => {
      if (!disposed) void load();
    });
    void listener.then((stop) => {
      if (disposed) stop();
    });
    return () => {
      disposed = true;
      void listener.then((stop) => stop()).catch(() => {});
    };
  }, [load]);

  async function sync(scope: GithubSyncScope) {
    if (syncing) return;
    setSyncing(true);
    setError(null);
    setNotice(null);
    try {
      const summary = await syncGithubRepo(repoId, scope);
      setNotice(
        summary.notModified
          ? "内容没有变化，仓库已是最新。"
          : `${scope === "full" ? "历史加载" : "同步"}完成，${summary.changedCount} 条内容有更新。`,
      );
      await load();
    } catch (syncError) {
      setError(syncError instanceof Error ? syncError.message : "同步失败，请重试。");
    } finally {
      setSyncing(false);
    }
  }

  async function saveMeta(description: string, note: string) {
    try {
      setRepo(
        await updateRepoMeta({
          id: repoId,
          customDescription: description || null,
          customNote: note || null,
        }),
      );
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : "保存失败，请重试。");
    }
  }

  const tabs = useMemo(
    () => [
      { id: "overview", label: "概览" },
      { id: "pulls", label: "Pull Requests", count: repo?.counts.openPullRequests },
      { id: "issues", label: "Issues", count: repo?.counts.openIssues },
      { id: "reviews", label: "评审", count: repo?.counts.reviews },
      { id: "releases", label: "Release", count: repo?.counts.releases },
      { id: "items", label: "我的条目", count: repo?.counts.items },
    ],
    [repo],
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <header className="border-b border-border bg-card px-5 py-4 sm:px-7">
        <button
          className="font-mono text-[11px] text-muted-foreground hover:text-foreground"
          type="button"
          onClick={onBack}
        >
          ← 返回仓库列表
        </button>
        {loading ? (
          <div className="mt-3 space-y-2" role="status" aria-label="正在加载仓库">
            <div className="h-6 w-1/3 animate-pulse rounded bg-muted" />
            <div className="h-3 w-2/3 animate-pulse rounded bg-muted/70" />
          </div>
        ) : repo ? (
          <>
            <div className="mt-2 flex flex-wrap items-start justify-between gap-4">
              <div className="min-w-0">
                <h2 className="text-xl font-bold tracking-tight break-words">
                  <span className="text-muted-foreground">{repo.owner}/</span>
                  {repo.name}
                </h2>
                <p className="mt-1 line-clamp-2 text-sm text-muted-foreground">
                  {repo.customDescription ?? repo.description ?? "还没有描述。"}
                </p>
              </div>
              <div className="flex flex-wrap gap-2">
                <button
                  className="h-8 rounded-lg border border-border px-3 text-sm text-foreground/80 transition hover:bg-muted disabled:opacity-50"
                  type="button"
                  disabled={syncing}
                  onClick={() => void sync("incremental")}
                >
                  {syncing ? "同步中…" : "同步"}
                </button>
                <button
                  className="h-8 rounded-lg border border-border px-3 text-sm text-muted-foreground transition hover:bg-muted hover:text-foreground disabled:opacity-50"
                  type="button"
                  title="拉取全部状态（含已关闭）的历史内容，不受增量游标限制"
                  disabled={syncing}
                  onClick={() => void sync("full")}
                >
                  加载历史
                </button>
              </div>
            </div>
            <p className="mt-2 font-mono text-[11px] text-muted-foreground">
              {repo.language ?? "—"} · ★ {formatCount(repo.stars)} · 最近同步{" "}
              {formatRelative(repo.lastSyncedAt)}
            </p>
          </>
        ) : null}

        {error ? (
          <p className="mt-3 rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
            {error}
          </p>
        ) : null}
        {notice ? (
          <p className="mt-3 rounded-md border border-success/30 bg-success/10 px-3 py-2 text-sm text-success">
            {notice}
          </p>
        ) : null}
      </header>

      <Tabs
        className="bg-card px-5 sm:px-7"
        items={tabs}
        value={tab}
        onChange={(id) => setTab(id as TabId)}
      />

      {repo && tab === "overview" ? <OverviewTab repo={repo} onSaveMeta={saveMeta} /> : null}
      {tab === "pulls" ? <ThreadsTab repoId={repoId} kind="pull_request" /> : null}
      {tab === "issues" ? <ThreadsTab repoId={repoId} kind="issue" /> : null}
      {tab === "reviews" ? (
        <div className="flex min-h-0 flex-1 flex-col">
          <div className="flex items-center gap-2 border-b border-border bg-card px-5 py-2 sm:px-7">
            {(
              [
                ["review_requested", "待我评审"],
                ["reviewed", "我评审过"],
              ] as const
            ).map(([value, label]) => (
              <button
                key={value}
                className={`h-7 rounded-md px-2.5 font-mono text-xs transition ${reviewScope === value ? "bg-accent text-link" : "text-muted-foreground hover:bg-accent hover:text-foreground"}`}
                type="button"
                onClick={() => setReviewScope(value)}
              >
                {label}
              </button>
            ))}
          </div>
          <ThreadsTab
            key={reviewScope}
            repoId={repoId}
            kind="pull_request"
            relationship={reviewScope}
          />
        </div>
      ) : null}
      {tab === "releases" ? <ReleasesTab repoId={repoId} /> : null}
      {tab === "items" ? <RepoItemsTab repoId={repoId} /> : null}
    </div>
  );
}

export default RepoDetail;
