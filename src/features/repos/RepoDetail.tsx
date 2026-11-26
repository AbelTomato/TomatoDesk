import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

import {
  Badge,
  Button,
  Input,
  ListItem,
  ListItemMeta,
  ListItemTitle,
  SectionTitle,
  StateMark,
  Tabs,
  Textarea,
  cn,
} from "../../components/ui";
import {
  archiveCapture,
  createCapture,
  getRepo,
  getThread,
  listCapturesPage,
  listRepoReleases,
  listRepoThreads,
  restoreCapture,
  syncGithubRepo,
  updateRepoMeta,
  type CaptureSummary,
  type GithubRelease,
  type GithubSyncScope,
  type RepoDetail as RepoDetailData,
  type ThreadDetail,
  type ThreadKind,
  type ThreadRelationship,
  type ThreadSummary,
} from "../../lib/api";
import { notifyCapturesChanged } from "../../lib/events";
import { formatCount, formatDate, formatRelative } from "../../lib/format";
import { openExternal } from "../../lib/openExternal";
import ThreadDetailPage from "./ThreadDetailPage";
import {
  relationLabel,
  reviewScopeLabels,
  stateLabels,
  stateTones,
  threadHeadline,
} from "./threadMeta";

type TabId = "overview" | "pulls" | "issues" | "reviews" | "releases" | "captures";

const PAGE_SIZE = 50;

/** 概览面板只放摘要：优先用同步时生成的 summary，缺失时从正文压平后截一段。 */
function overviewText(thread: ThreadDetail): string | null {
  const summary = thread.summary?.trim();
  if (summary) return summary;
  const body = thread.body?.replace(/\s+/g, " ").trim();
  if (!body) return null;
  const chars = Array.from(body);
  return chars.length > 200 ? `${chars.slice(0, 200).join("")}…` : body;
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

  return multiline ? (
    <Textarea
      value={draft}
      placeholder={placeholder}
      disabled={saving}
      onChange={(event) => setDraft(event.target.value)}
      onBlur={() => void commit()}
    />
  ) : (
    <Input
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
    <div className="grid gap-8 px-5 py-6 sm:px-7 lg:grid-cols-[minmax(0,1.6fr)_minmax(15rem,1fr)]">
      <div className="space-y-7">
        <section>
          <SectionTitle>描述</SectionTitle>
          <p className="mt-2 text-sm leading-7 whitespace-pre-wrap">
            {repo.description ?? "仓库没有填写描述。"}
          </p>
        </section>

        <section>
          <div className="flex items-baseline justify-between gap-3 border-b border-border pb-2">
            <h3 className="text-sm font-bold tracking-tight">README 摘要</h3>
            {repo.readmeText ? (
              <button
                className="font-mono text-xs text-link hover:underline"
                type="button"
                onClick={() => setShowReadme((current) => !current)}
              >
                {showReadme ? "收起原文" : "查看原文"}
              </button>
            ) : null}
          </div>
          {repo.readmeSummary ? (
            <p className="mt-2 text-sm leading-7">{repo.readmeSummary}</p>
          ) : (
            <p className="mt-2 text-sm text-muted-foreground">
              还没有 README 摘要，同步后会从 README 生成纯文本摘要。
            </p>
          )}
          {showReadme && repo.readmeText ? (
            <pre className="mt-3 max-h-96 overflow-auto rounded-lg bg-background p-3 font-mono text-xs leading-6 whitespace-pre-wrap text-muted-foreground">
              {repo.readmeText}
            </pre>
          ) : null}
        </section>

        <section>
          <SectionTitle>自定义描述</SectionTitle>
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
          <SectionTitle>备注</SectionTitle>
          <div className="mt-2">
            <InlineEdit
              value={repo.customNote}
              placeholder="记录约定、分支策略、发布节奏…"
              multiline
              onSave={(next) => onSaveMeta(repo.customDescription ?? "", next)}
            />
          </div>
        </section>
      </div>

      <aside className="space-y-6">
        <section>
          <SectionTitle>仓库信息</SectionTitle>
          <dl className="divide-y divide-border text-sm">
            {[
              ["语言", repo.language ?? "—"],
              ["Stars", formatCount(repo.stars)],
              ["Forks", formatCount(repo.forks)],
              ["Open issues", formatCount(repo.openIssues)],
              ["默认分支", repo.defaultBranch ?? "—"],
              ["最近推送", formatRelative(repo.pushedAt)],
              ["最近同步", formatRelative(repo.metaSyncedAt)],
            ].map(([label, value]) => (
              <div key={label} className="flex items-baseline justify-between gap-3 py-1.5">
                <dt className="text-muted-foreground">{label}</dt>
                <dd className="font-mono text-xs">{value}</dd>
              </div>
            ))}
          </dl>
        </section>

        <section>
          <SectionTitle>进行中</SectionTitle>
          <dl className="divide-y divide-border text-sm">
            {[
              ["PR", repo.counts.openPullRequests],
              ["Issue", repo.counts.openIssues],
              ["评审", repo.counts.reviews],
              ["Release", repo.counts.releases],
              ["相关随手记", repo.counts.captures],
            ].map(([label, value]) => (
              <div key={label} className="flex items-baseline justify-between gap-3 py-1.5">
                <dt className="text-muted-foreground">{label}</dt>
                <dd className="font-mono text-xs">{value}</dd>
              </div>
            ))}
          </dl>
        </section>

        {homepage ? (
          <button
            className="block w-full truncate text-left font-mono text-xs break-all text-link hover:underline"
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
  leading,
  defaultState = "open",
  onOpenThread,
}: {
  repoId: string;
  kind: ThreadKind;
  relationship?: ThreadRelationship;
  /** 与状态筛选同排的附加筛选（评审范围），用竖线分隔。 */
  leading?: React.ReactNode;
  defaultState?: "open" | "all";
  onOpenThread: (id: string) => void;
}) {
  const [stateFilter, setStateFilter] = useState<"open" | "all">(defaultState);
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
      <div className="flex items-center justify-between gap-3 border-b border-border px-5 py-1.5 sm:px-7">
        <div className="flex items-center gap-1">
          {leading}
          {leading ? <span aria-hidden className="mx-1.5 h-3 w-px bg-border" /> : null}
          {(["open", "all"] as const).map((value) => (
            <button
              key={value}
              className={cn(
                "h-7 rounded-md px-2 font-mono text-xs transition-colors",
                stateFilter === value ? "text-link" : "text-muted-foreground hover:text-foreground",
              )}
              type="button"
              onClick={() => setStateFilter(value)}
            >
              {value}
            </button>
          ))}
        </div>
        <span className="font-mono text-xs text-muted-foreground">
          {threads.length} 条{hasMore ? " +" : ""}
        </span>
      </div>

      <div className="grid min-h-0 flex-1 lg:grid-cols-[minmax(20rem,1.1fr)_minmax(20rem,0.9fr)]">
        <div className="min-h-0 overflow-y-auto border-b border-border lg:border-r lg:border-b-0">
          {loading ? (
            <div className="divide-y divide-border" role="status" aria-label="正在加载">
              {Array.from({ length: 5 }).map((_, index) => (
                <div key={index} className="space-y-2 px-5 py-3 sm:px-7" aria-hidden>
                  <div className="h-3.5 w-3/4 animate-pulse rounded bg-muted" />
                  <div className="h-3 w-1/3 animate-pulse rounded bg-muted/70" />
                </div>
              ))}
            </div>
          ) : null}
          {!loading && error ? (
            <div className="mx-4 callout callout-error px-4 py-3 text-sm text-destructive">
              <p>{error}</p>
              <button
                className="mt-2 text-xs font-medium underline"
                type="button"
                onClick={() => void loadFirstPage()}
              >
                重新加载
              </button>
            </div>
          ) : null}
          {!loading && !error && threads.length === 0 ? (
            <p className="px-5 py-10 text-center text-sm text-muted-foreground sm:px-7">
              没有符合条件的内容，同步一次或切换筛选试试。
            </p>
          ) : null}

          <div className="divide-y divide-border">
            {threads.map((thread) => {
              const relation = relationLabel(thread);
              return (
                <ListItem
                  key={thread.id}
                  active={thread.id === selectedId}
                  flush
                  className="px-5 py-3 sm:px-7"
                  aria-current={thread.id === selectedId ? "true" : undefined}
                  onClick={() => setSelectedId(thread.id)}
                >
                  <span className="flex items-start justify-between gap-3">
                    <ListItemTitle>{thread.title}</ListItemTitle>
                    <StateMark tone={stateTones[thread.state]} className="shrink-0 pt-0.5">
                      {stateLabels[thread.state]}
                    </StateMark>
                  </span>
                  <ListItemMeta>
                    <span>#{thread.number}</span>
                    {relation ? <span>{relation}</span> : null}
                    {thread.kind === "pull_request" && thread.changedFiles !== null ? (
                      <span>
                        +{thread.additions ?? 0} −{thread.deletions ?? 0}
                      </span>
                    ) : null}
                    <span>{thread.commentCount} comments</span>
                    <span className="ml-auto">{formatRelative(thread.updatedAt)}</span>
                  </ListItemMeta>
                </ListItem>
              );
            })}
          </div>

          {!loading && !error ? (
            <div className="flex justify-center px-4 py-4">
              {loadingMore ? (
                <span className="font-mono text-xs text-muted-foreground">正在加载更多…</span>
              ) : hasMore ? (
                <Button variant="outline" size="sm" onClick={() => void loadMore()}>
                  加载更多
                </Button>
              ) : threads.length > 0 ? (
                <span className="font-mono text-xs text-muted-foreground">已显示全部</span>
              ) : null}
            </div>
          ) : null}
        </div>

        <div className="min-w-0 overflow-y-auto bg-card px-5 py-6 sm:px-7">
          {detail ? (
            <div className="mx-auto max-w-2xl">
              <div className="flex flex-wrap items-center gap-3 font-mono text-xs text-muted-foreground">
                <span>{threadHeadline(detail)}</span>
                <StateMark tone={stateTones[detail.state]}>{stateLabels[detail.state]}</StateMark>
                {relationLabel(detail) ? <span>{relationLabel(detail)}</span> : null}
                <span className="ml-auto">{detail.commentCount} comments</span>
              </div>
              <h3 className="mt-2 text-xl leading-snug font-bold tracking-tight break-words">
                {detail.title}
              </h3>
              <p className="mt-1 font-mono text-xs text-muted-foreground">
                {detail.author ?? "未知作者"} · 更新于 {formatDate(detail.updatedAt)}
                {detail.mergedAt ? ` · 合并于 ${formatDate(detail.mergedAt)}` : ""}
              </p>

              <div className="mt-4 flex flex-wrap items-center gap-2">
                <Button size="sm" onClick={() => onOpenThread(detail.id)}>
                  查看详情
                </Button>
                <Button variant="outline" size="sm" onClick={() => void openExternal(detail.url)}>
                  在 GitHub 打开
                </Button>
              </div>

              {detail.kind === "pull_request" &&
              (detail.additions !== null || detail.deletions !== null) ? (
                <p className="mt-3 font-mono text-xs text-muted-foreground">
                  +{detail.additions ?? 0} −{detail.deletions ?? 0}
                  {detail.changedFiles !== null ? ` · ${detail.changedFiles} files changed` : ""}
                </p>
              ) : null}

              <div className="mt-6">
                <p className="text-xs font-medium text-muted-foreground">摘要</p>
                {detailLoading ? (
                  <div className="mt-2 space-y-2" role="status" aria-label="正在加载摘要">
                    {Array.from({ length: 3 }).map((_, index) => (
                      <div
                        key={index}
                        className="h-3 animate-pulse rounded bg-muted"
                        style={{ width: `${90 - index * 15}%` }}
                      />
                    ))}
                  </div>
                ) : overviewText(detail) ? (
                  <p className="mt-2 line-clamp-6 text-sm leading-7">{overviewText(detail)}</p>
                ) : (
                  <p className="mt-2 text-sm text-muted-foreground">这条内容没有正文。</p>
                )}
              </div>

              <button
                className="mt-3 font-mono text-xs text-link hover:underline"
                type="button"
                onClick={() => onOpenThread(detail.id)}
              >
                查看完整内容 →
              </button>
            </div>
          ) : (
            <div className="flex min-h-48 items-center justify-center text-center text-sm text-muted-foreground">
              选择一条内容查看概览
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
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className="divide-y divide-border">
        {releases.map((release) => (
          <article key={release.id} className="px-5 py-4 sm:px-7">
            <div className="flex flex-wrap items-baseline gap-2">
              <span className="font-mono text-sm text-link">{release.tagName}</span>
              {release.name ? <span className="text-sm font-medium">{release.name}</span> : null}
              {release.isPrerelease ? <Badge>pre-release</Badge> : null}
              {release.isDraft ? <Badge>draft</Badge> : null}
              <time className="ml-auto font-mono text-xs text-muted-foreground">
                {formatDate(release.publishedAt ?? release.createdAt)}
              </time>
            </div>
            {release.body ? (
              <>
                <p
                  className={cn(
                    "mt-2 text-sm leading-6 whitespace-pre-wrap text-muted-foreground",
                    expanded === release.id ? "" : "line-clamp-3",
                  )}
                >
                  {release.body}
                </p>
                <div className="mt-2 flex gap-3">
                  <button
                    className="font-mono text-xs text-link hover:underline"
                    type="button"
                    onClick={() =>
                      setExpanded((current) => (current === release.id ? null : release.id))
                    }
                  >
                    {expanded === release.id ? "收起" : "展开全文"}
                  </button>
                  <button
                    className="font-mono text-xs text-muted-foreground hover:text-foreground hover:underline"
                    type="button"
                    onClick={() => void openExternal(release.url)}
                  >
                    在 GitHub 打开
                  </button>
                </div>
              </>
            ) : null}
          </article>
        ))}
      </div>
    </div>
  );
}

function RepoCapturesTab({ repoId }: { repoId: string }) {
  const [captures, setCaptures] = useState<CaptureSummary[]>([]);
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
      const page = await listCapturesPage({ repoId, limit: PAGE_SIZE });
      setCaptures(page.captures);
      setCursor(page.nextCursor);
      setHasMore(page.hasMore);
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : "加载随手记失败。");
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
      const page = await listCapturesPage({ repoId, cursor, limit: PAGE_SIZE });
      setCaptures((current) => {
        const seen = new Set(current.map((capture) => capture.id));
        return [...current, ...page.captures.filter((capture) => !seen.has(capture.id))];
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
      await createCapture({
        kind: "note",
        source: "repo",
        url: null,
        title: null,
        summary: null,
        note,
        body: null,
        repoId,
      });
      setDraft("");
      await load();
      notifyCapturesChanged();
    } catch (createError) {
      setError(createError instanceof Error ? createError.message : "添加随手记失败。");
    } finally {
      setBusy(false);
    }
  }

  async function toggleArchived(capture: CaptureSummary) {
    if (busy) return;
    setBusy(true);
    try {
      const updated = capture.archivedAt
        ? await restoreCapture(capture.id)
        : await archiveCapture(capture.id);
      setCaptures((current) =>
        current.map((entry) => (entry.id === updated.id ? { ...entry, ...updated } : entry)),
      );
      notifyCapturesChanged();
    } catch (updateError) {
      setError(updateError instanceof Error ? updateError.message : "更新归档状态失败。");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className="px-5 py-5 sm:px-7">
        <form className="flex flex-col gap-2 sm:flex-row" onSubmit={addNote}>
          <Textarea
            className="flex-1"
            value={draft}
            placeholder="记一条与这个仓库相关的笔记…"
            maxLength={16_384}
            onChange={(event) => setDraft(event.target.value)}
          />
          <Button type="submit" className="self-start" disabled={busy || !draft.trim()}>
            添加随手记
          </Button>
        </form>

        {error ? <p className="mt-3 text-sm text-destructive">{error}</p> : null}
      </div>

      {loading ? (
        <p className="px-5 py-6 font-mono text-xs text-muted-foreground sm:px-7">正在加载…</p>
      ) : captures.length === 0 ? (
        <p className="px-5 py-6 text-sm text-muted-foreground sm:px-7">
          还没有挂在这个仓库下的随手记，可以在上面记一条。
        </p>
      ) : (
        <div className="divide-y divide-border border-t border-border">
          {captures.map((capture) => (
            <article key={capture.id} className="px-5 py-3 sm:px-7">
              <p className="text-sm leading-6 whitespace-pre-wrap">
                {capture.title ?? capture.note ?? capture.summary ?? capture.source}
              </p>
              <div className="mt-1.5 flex flex-wrap items-center gap-3">
                {capture.archivedAt ? (
                  <span className="font-mono text-xs text-muted-foreground">已归档</span>
                ) : null}
                <time className="font-mono text-xs text-muted-foreground">
                  {formatRelative(capture.updatedAt)}
                </time>
                <div className="ml-auto flex items-center gap-1">
                  <button
                    className={cn(
                      "h-6 rounded px-1.5 font-mono text-xs transition-colors",
                      capture.archivedAt
                        ? "text-link"
                        : "text-muted-foreground hover:text-foreground",
                    )}
                    type="button"
                    disabled={busy}
                    onClick={() => void toggleArchived(capture)}
                  >
                    {capture.archivedAt ? "取消归档" : "归档"}
                  </button>
                </div>
              </div>
            </article>
          ))}
        </div>
      )}

      {hasMore ? (
        <div className="flex justify-center px-4 py-4">
          <Button variant="outline" size="sm" disabled={busy} onClick={() => void loadMore()}>
            加载更多
          </Button>
        </div>
      ) : null}
    </div>
  );
}

function RepoDetail({
  repoId,
  initialThreadId,
  onBack,
}: {
  repoId: string;
  /** 首屏 `?thread=` 直接打开某条详情，由外壳一次性传入。 */
  initialThreadId: string | null;
  onBack: () => void;
}) {
  const [repo, setRepo] = useState<RepoDetailData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [syncing, setSyncing] = useState(false);
  const [tab, setTab] = useState<TabId>("overview");
  const [reviewScope, setReviewScope] = useState<ThreadRelationship>("review_requested");
  /** 非空时整块内容区被详情页覆盖，列表与筛选状态原地保留，返回即可续上。 */
  const [openThreadId, setOpenThreadId] = useState<string | null>(initialThreadId);

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
      { id: "captures", label: "相关随手记", count: repo?.counts.captures },
    ],
    [repo],
  );

  const repoLabel = repo ? `${repo.owner}/${repo.name}` : null;

  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      {/* 详情页覆盖在上面时，底层只保留观感，键盘焦点一并挡住。 */}
      <div className="flex min-h-0 flex-1 flex-col" inert={openThreadId !== null}>
        <header className="px-5 pt-3 sm:px-7">
          <button
            className="font-mono text-xs text-muted-foreground transition-colors hover:text-foreground"
            type="button"
            onClick={onBack}
          >
            ← 仓库列表
          </button>
          {loading ? (
            <div className="mt-3 space-y-2 pb-4" role="status" aria-label="正在加载仓库">
              <div className="h-5 w-1/3 animate-pulse rounded bg-muted" />
              <div className="h-3 w-2/3 animate-pulse rounded bg-muted/70" />
            </div>
          ) : repo ? (
            <>
              <div className="mt-2 flex flex-wrap items-start justify-between gap-4">
                <div className="min-w-0">
                  <h2 className="text-lg font-bold tracking-tight break-words">
                    <span className="text-muted-foreground">{repo.owner}/</span>
                    {repo.name}
                  </h2>
                  <p className="mt-1 line-clamp-2 text-sm text-muted-foreground">
                    {repo.customDescription ?? repo.description ?? "还没有描述。"}
                  </p>
                </div>
                <div className="flex flex-wrap items-center gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={syncing}
                    onClick={() => void sync("incremental")}
                  >
                    {syncing ? "同步中…" : "同步"}
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    title="拉取全部状态（含已关闭）的历史内容，不受增量游标限制"
                    disabled={syncing}
                    onClick={() => void sync("full")}
                  >
                    加载历史
                  </Button>
                </div>
              </div>
              <p className="mt-1.5 font-mono text-xs text-muted-foreground">
                {repo.language ?? "—"} · ★ {formatCount(repo.stars)} · 最近同步{" "}
                {formatRelative(repo.lastSyncedAt)}
              </p>
            </>
          ) : null}

          {error ? (
            <p className="mt-3 callout callout-error px-3 py-2 text-sm text-destructive">{error}</p>
          ) : null}
          {notice ? (
            <p className="mt-3 callout callout-success px-3 py-2 text-sm text-success">{notice}</p>
          ) : null}

          <div className="mt-3">
            <Tabs items={tabs} value={tab} onChange={(id) => setTab(id as TabId)} />
          </div>
        </header>

        {repo && tab === "overview" ? <OverviewTab repo={repo} onSaveMeta={saveMeta} /> : null}
        {tab === "pulls" ? (
          <ThreadsTab repoId={repoId} kind="pull_request" onOpenThread={setOpenThreadId} />
        ) : null}
        {tab === "issues" ? (
          <ThreadsTab repoId={repoId} kind="issue" onOpenThread={setOpenThreadId} />
        ) : null}
        {tab === "reviews" ? (
          <ThreadsTab
            key={reviewScope}
            repoId={repoId}
            kind="pull_request"
            relationship={reviewScope}
            // 评审过的 PR 多半已合并或关闭，默认放开状态筛选才看得见。
            defaultState={reviewScope === "reviewed" ? "all" : "open"}
            onOpenThread={setOpenThreadId}
            leading={(["review_requested", "reviewed"] as const).map((value) => (
              <button
                key={value}
                className={cn(
                  "h-7 rounded-md px-2 font-mono text-xs transition-colors",
                  reviewScope === value
                    ? "text-link"
                    : "text-muted-foreground hover:text-foreground",
                )}
                type="button"
                onClick={() => setReviewScope(value)}
              >
                {reviewScopeLabels[value]}
              </button>
            ))}
          />
        ) : null}
        {tab === "releases" ? <ReleasesTab repoId={repoId} /> : null}
        {tab === "captures" ? <RepoCapturesTab repoId={repoId} /> : null}
      </div>

      {openThreadId ? (
        <div className="absolute inset-0 z-10 flex flex-col">
          <ThreadDetailPage
            threadId={openThreadId}
            repoLabel={repoLabel}
            onBack={() => setOpenThreadId(null)}
          />
        </div>
      ) : null}
    </div>
  );
}

export default RepoDetail;
