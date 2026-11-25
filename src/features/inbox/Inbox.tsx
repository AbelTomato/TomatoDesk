import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openUrl } from "@tauri-apps/plugin-opener";
import GithubPanel from "../github/GithubPanel";
import UpdateBanner from "../update/UpdateBanner";

import {
  countItemsByStatus,
  createTag,
  exportItemsJson,
  exportItemsMarkdown,
  getItemBody,
  getItemTags,
  listItemsPage,
  listTags,
  openCapture,
  searchItems,
  setItemTags,
  updateItem,
  type ItemBody,
  type ItemKind,
  type ItemSort,
  type ItemStatus,
  type ItemStatusCounts,
  type ItemSummary,
  type Tag,
} from "../../lib/api";
import { appEnvironment } from "../../lib/env";

type StatusFilter = "all" | ItemStatus;
type KindFilter = "all" | ItemKind;

const PAGE_SIZE = 50;
const SEARCH_LIMIT = 100;

const emptyCounts: ItemStatusCounts = { all: 0, pending: 0, processed: 0, archived: 0 };

const statusMeta: Record<ItemStatus, { label: string; tone: string }> = {
  pending: { label: "待处理", tone: "bg-muted text-foreground" },
  processed: { label: "已处理", tone: "bg-success/10 text-success" },
  archived: { label: "已归档", tone: "bg-muted text-muted-foreground" },
};

const kindMeta: Record<ItemKind, { label: string; tone: string }> = {
  note: { label: "笔记", tone: "bg-muted text-muted-foreground" },
  link: { label: "链接", tone: "bg-muted text-muted-foreground" },
  github: { label: "GitHub", tone: "bg-muted text-muted-foreground" },
};

function formatDate(value: string) {
  return new Intl.DateTimeFormat("zh-CN", {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}

function getItemLabel(item: ItemSummary) {
  return item.title?.trim() || item.summary?.trim() || item.note?.trim() || item.source;
}

function describeItem(item: ItemSummary) {
  if (item.summary?.trim()) return item.summary.trim();
  if (item.note?.trim()) return item.note.trim();
  return item.url || item.source;
}

function Inbox() {
  const [items, setItems] = useState<ItemSummary[]>([]);
  const [tags, setTags] = useState<Tag[]>([]);
  const [counts, setCounts] = useState<ItemStatusCounts>(emptyCounts);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [selectedTags, setSelectedTags] = useState<Tag[]>([]);
  const [detailBody, setDetailBody] = useState<ItemBody | null>(null);
  const [bodyLoading, setBodyLoading] = useState(false);
  const [bodyError, setBodyError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("pending");
  const [kindFilter, setKindFilter] = useState<KindFilter>("all");
  const [sortOrder, setSortOrder] = useState<ItemSort>("updated");
  const [cursor, setCursor] = useState<string | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [initialLoading, setInitialLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [mutating, setMutating] = useState(false);
  const [tagSaving, setTagSaving] = useState(false);
  const [newTagName, setNewTagName] = useState("");
  const [showGithub, setShowGithub] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [exportMessage, setExportMessage] = useState<string | null>(null);
  const [exportFailed, setExportFailed] = useState(false);
  const requestId = useRef(0);
  const searchRef = useRef<HTMLInputElement | null>(null);
  const sentinelRef = useRef<HTMLDivElement | null>(null);

  const searching = query.trim().length > 0;

  const filterInput = useMemo(
    () => ({
      status: statusFilter === "all" ? null : statusFilter,
      kind: kindFilter === "all" ? null : kindFilter,
      sort: sortOrder,
    }),
    [kindFilter, sortOrder, statusFilter],
  );

  const refreshCounts = useCallback(async (kind: KindFilter) => {
    try {
      setCounts(await countItemsByStatus(kind === "all" ? undefined : kind));
    } catch {
      // 计数失败不影响列表，保留上一次的角标即可。
    }
  }, []);

  /** 首屏只取 50 条；搜索走 FTS，最多 100 条且不分页。 */
  const loadFirstPage = useCallback(async () => {
    const currentRequest = ++requestId.current;
    const keyword = query.trim();
    setInitialLoading(true);
    setError(null);
    try {
      const [nextTags, nextCounts] = await Promise.all([
        listTags(),
        countItemsByStatus(kindFilter === "all" ? undefined : kindFilter),
      ]);
      if (currentRequest !== requestId.current) return;
      setTags(nextTags);
      setCounts(nextCounts);

      if (keyword) {
        const results = await searchItems(keyword);
        if (currentRequest !== requestId.current) return;
        setItems(results.slice(0, SEARCH_LIMIT));
        setCursor(null);
        setHasMore(false);
      } else {
        const page = await listItemsPage({ ...filterInput, limit: PAGE_SIZE });
        if (currentRequest !== requestId.current) return;
        setItems(page.items);
        setCursor(page.nextCursor);
        setHasMore(page.hasMore);
      }
    } catch (loadError) {
      if (currentRequest !== requestId.current) return;
      setItems([]);
      setCursor(null);
      setHasMore(false);
      setError(loadError instanceof Error ? loadError.message : "加载 Inbox 失败，请重试。");
    } finally {
      if (currentRequest === requestId.current) setInitialLoading(false);
    }
  }, [filterInput, kindFilter, query]);

  const loadMore = useCallback(async () => {
    if (!hasMore || loadingMore || initialLoading || searching || !cursor) return;
    const currentRequest = requestId.current;
    setLoadingMore(true);
    try {
      const page = await listItemsPage({ ...filterInput, cursor, limit: PAGE_SIZE });
      if (currentRequest !== requestId.current) return;
      setItems((current) => {
        const seen = new Set(current.map((item) => item.id));
        return [...current, ...page.items.filter((item) => !seen.has(item.id))];
      });
      setCursor(page.nextCursor);
      setHasMore(page.hasMore);
    } catch (loadError) {
      if (currentRequest !== requestId.current) return;
      setError(loadError instanceof Error ? loadError.message : "加载更多失败，请重试。");
    } finally {
      setLoadingMore(false);
    }
  }, [cursor, filterInput, hasMore, initialLoading, loadingMore, searching]);

  useEffect(() => {
    const timer = window.setTimeout(() => void loadFirstPage(), 220);
    return () => window.clearTimeout(timer);
  }, [loadFirstPage]);

  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    const listener = getCurrentWindow().listen("github-sync-updated", () => {
      if (!disposed) void loadFirstPage();
    });
    void listener.then((stop) => {
      if (disposed) stop();
    });
    return () => {
      disposed = true;
      void listener.then((stop) => stop()).catch(() => {});
    };
  }, [loadFirstPage]);

  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    const listener = getCurrentWindow().listen("quick-capture-completed", () => {
      if (!disposed) void loadFirstPage();
    });
    void listener.then((stop) => {
      if (disposed) stop();
    });
    return () => {
      disposed = true;
      void listener.then((stop) => stop()).catch(() => {});
    };
  }, [loadFirstPage]);

  useEffect(() => {
    function focusSearch(event: KeyboardEvent) {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
      }
    }
    window.addEventListener("keydown", focusSearch);
    return () => window.removeEventListener("keydown", focusSearch);
  }, []);

  // 滚动到列表底部时增量加载下一页。
  useEffect(() => {
    const target = sentinelRef.current;
    if (!target || !hasMore || searching) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) void loadMore();
      },
      { rootMargin: "240px 0px" },
    );
    observer.observe(target);
    return () => observer.disconnect();
  }, [hasMore, loadMore, searching]);

  const selectedItem = items.find((item) => item.id === selectedId) ?? null;

  // 详情正文按需加载：切换选中项时才请求，列表页不携带全文。
  useEffect(() => {
    if (!selectedId) {
      setDetailBody(null);
      setBodyError(null);
      setBodyLoading(false);
      return;
    }
    let disposed = false;
    setBodyLoading(true);
    setBodyError(null);
    setDetailBody(null);
    void getItemBody(selectedId)
      .then((body) => {
        if (!disposed) setDetailBody(body);
      })
      .catch((loadError) => {
        if (disposed) return;
        setBodyError(loadError instanceof Error ? loadError.message : "正文加载失败。");
      })
      .finally(() => {
        if (!disposed) setBodyLoading(false);
      });
    return () => {
      disposed = true;
    };
  }, [selectedId]);

  useEffect(() => {
    if (!selectedId) {
      setSelectedTags([]);
      return;
    }
    let disposed = false;
    void getItemTags(selectedId)
      .then((nextTags) => {
        if (!disposed) setSelectedTags(nextTags);
      })
      .catch(() => {
        if (!disposed) setSelectedTags([]);
      });
    return () => {
      disposed = true;
    };
  }, [selectedId]);

  const visibleItems = useMemo(() => {
    // 分页与排序都在 SQL 里完成；只有搜索结果（≤100 条）在前端补筛。
    if (!searching) return items;
    return items
      .filter((item) => statusFilter === "all" || item.status === statusFilter)
      .filter((item) => kindFilter === "all" || item.kind === kindFilter);
  }, [items, kindFilter, searching, statusFilter]);

  useEffect(() => {
    if (visibleItems.length === 0) {
      if (selectedId !== null) setSelectedId(null);
      return;
    }
    if (!visibleItems.some((item) => item.id === selectedId)) {
      setSelectedId(visibleItems[0].id);
    }
  }, [selectedId, visibleItems]);

  async function changeStatus(status: ItemStatus) {
    if (!selectedItem || selectedItem.status === status || mutating) return;
    setMutating(true);
    setError(null);
    try {
      const updated = await updateItem({ ...selectedItem, status });
      // 筛选在后端完成，改完状态后不再匹配当前筛选的行要就地移除。
      const stillMatches = statusFilter === "all" || updated.status === statusFilter;
      setItems((current) =>
        stillMatches
          ? current.map((item) => (item.id === updated.id ? { ...item, ...updated } : item))
          : current.filter((item) => item.id !== updated.id),
      );
      void refreshCounts(kindFilter);
    } catch (updateError) {
      setError(updateError instanceof Error ? updateError.message : "更新状态失败，请重试。");
    } finally {
      setMutating(false);
    }
  }

  async function toggleTag(tagId: string) {
    if (!selectedItem || tagSaving) return;
    const currentIds = new Set(selectedTags.map((tag) => tag.id));
    if (currentIds.has(tagId)) currentIds.delete(tagId);
    else currentIds.add(tagId);
    setTagSaving(true);
    setError(null);
    try {
      setSelectedTags(await setItemTags(selectedItem.id, [...currentIds]));
    } catch (tagError) {
      setError(tagError instanceof Error ? tagError.message : "更新标签失败，请重试。");
    } finally {
      setTagSaving(false);
    }
  }

  async function addTag(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const name = newTagName.trim();
    if (!name || !selectedItem || tagSaving) return;
    setTagSaving(true);
    setError(null);
    try {
      const created = await createTag({ name, color: null });
      const nextTags = [...tags.filter((tag) => tag.id !== created.id), created].sort(
        (left, right) => left.name.localeCompare(right.name, "zh-CN"),
      );
      setTags(nextTags);
      setSelectedTags(
        await setItemTags(selectedItem.id, [
          ...new Set([...selectedTags.map((tag) => tag.id), created.id]),
        ]),
      );
      setNewTagName("");
    } catch (tagError) {
      setError(tagError instanceof Error ? tagError.message : "创建标签失败，请重试。");
    } finally {
      setTagSaving(false);
    }
  }

  async function openSelectedLink() {
    if (!selectedItem?.url) return;
    try {
      if (isTauri()) await openUrl(selectedItem.url);
      else window.open(selectedItem.url, "_blank", "noopener,noreferrer");
    } catch {
      setError("打开链接失败，请复制 URL 后重试。");
    }
  }

  async function exportData(format: "json" | "markdown") {
    if (exporting) return;
    setExporting(true);
    setExportMessage(null);
    setExportFailed(false);
    try {
      const result = format === "json" ? await exportItemsJson() : await exportItemsMarkdown();
      if (result) setExportMessage(`已导出 ${result.itemCount} 条内容。`);
    } catch (exportError) {
      setExportFailed(true);
      setExportMessage(exportError instanceof Error ? exportError.message : "导出失败，请重试。");
    } finally {
      setExporting(false);
    }
  }

  return (
    <main className="min-h-screen bg-background text-foreground">
      <div className="mx-auto flex min-h-screen max-w-[1500px] flex-col lg:flex-row">
        <aside className="flex w-full shrink-0 flex-col border-b border-border bg-card px-5 py-5 lg:w-64 lg:border-r lg:border-b-0 lg:px-6 lg:py-7">
          <div className="flex items-center justify-between gap-4 lg:block">
            <div>
              <p className="text-xs font-medium tracking-wide text-link">TomatoDesk</p>
              <h1 className="mt-1 text-xl font-bold tracking-tight">个人收件箱</h1>
            </div>
            <span className="rounded border border-border px-2 py-0.5 font-mono text-[10px] uppercase tracking-wider text-muted-foreground">
              {appEnvironment}
            </span>
          </div>

          <button
            className="mt-6 flex h-8 items-center justify-center rounded-lg bg-primary px-3 text-sm font-medium text-primary-foreground transition hover:bg-primary/90 active:translate-y-px focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
            type="button"
            onClick={() => void openCapture()}
          >
            快速收集
          </button>

          <nav className="mt-7 grid grid-cols-2 gap-2 lg:grid-cols-1" aria-label="主导航">
            <button
              className={`flex items-center justify-between rounded-md px-3 py-2 text-left text-sm ${showGithub ? "text-muted-foreground hover:bg-accent hover:text-foreground" : "bg-accent font-medium text-link"}`}
              type="button"
              onClick={() => setShowGithub(false)}
            >
              <span>Inbox</span>
              <span className="font-mono text-xs text-muted-foreground">{counts.pending}</span>
            </button>
            <button
              className={`flex items-center justify-between rounded-md px-3 py-2 text-left text-sm ${showGithub ? "bg-accent font-medium text-link" : "text-muted-foreground hover:bg-accent hover:text-foreground"}`}
              type="button"
              onClick={() => setShowGithub(true)}
            >
              <span>GitHub</span>
              <span className="font-mono text-xs text-muted-foreground">同步</span>
            </button>
          </nav>

          <section className="mt-7 border-t border-border pt-5" aria-label="导出数据">
            <h2 className="text-sm font-medium">导出数据</h2>
            <div className="mt-3 grid grid-cols-2 gap-2 lg:grid-cols-1">
              <button
                className="h-8 rounded-lg border border-border px-3 text-sm text-foreground/80 transition hover:bg-muted disabled:opacity-50"
                type="button"
                disabled={exporting}
                onClick={() => void exportData("json")}
              >
                导出 JSON
              </button>
              <button
                className="h-8 rounded-lg border border-border px-3 text-sm text-foreground/80 transition hover:bg-muted disabled:opacity-50"
                type="button"
                disabled={exporting}
                onClick={() => void exportData("markdown")}
              >
                导出 Markdown
              </button>
            </div>
            {exportMessage ? (
              <p
                className={`mt-2 break-words text-xs leading-5 ${exportFailed ? "text-destructive" : "text-success"}`}
                role={exportFailed ? "alert" : "status"}
              >
                {exportMessage}
              </p>
            ) : null}
          </section>

          <div className="mt-auto hidden border-t border-border pt-5 font-mono text-xs leading-5 text-muted-foreground lg:block">
            <p>Ctrl + K 聚焦搜索</p>
            <p>数据保存在本机 SQLite</p>
          </div>
        </aside>

        <section className="flex min-w-0 flex-1 flex-col">
          <UpdateBanner />
          {showGithub ? (
            <GithubPanel onSynced={() => void loadFirstPage()} />
          ) : (
            <>
              <header className="border-b border-border bg-card px-5 py-5 sm:px-7">
                <div className="flex flex-col gap-4 xl:flex-row xl:items-center xl:justify-between">
                  <div>
                    <p className="text-sm text-muted-foreground">Inbox</p>
                    <h2 className="mt-1 text-2xl font-bold tracking-tight">找到要处理的内容</h2>
                  </div>
                  <div className="flex min-w-0 flex-1 gap-2 xl:max-w-2xl">
                    <label className="relative min-w-0 flex-1">
                      <span className="sr-only">搜索标题、正文和来源</span>
                      <input
                        ref={searchRef}
                        className="h-8 w-full rounded-lg border border-input bg-transparent px-2.5 pr-16 text-sm outline-none transition placeholder:text-muted-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
                        type="search"
                        value={query}
                        placeholder="搜索标题、正文和来源"
                        onChange={(event) => setQuery(event.target.value)}
                      />
                      <kbd className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 rounded border border-border bg-card px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground">
                        Ctrl K
                      </kbd>
                    </label>
                    <button
                      className="h-8 shrink-0 rounded-lg border border-border px-3 text-sm text-foreground/80 transition hover:bg-muted"
                      type="button"
                      onClick={() => void loadFirstPage()}
                    >
                      刷新
                    </button>
                  </div>
                </div>

                <div className="mt-5 flex flex-wrap items-center gap-2">
                  {(["all", "pending", "processed", "archived"] as const).map((status) => (
                    <button
                      key={status}
                      className={`h-7 rounded-md px-2.5 font-mono text-xs transition ${statusFilter === status ? "bg-primary text-primary-foreground" : "text-muted-foreground hover:bg-accent hover:text-foreground"}`}
                      type="button"
                      onClick={() => setStatusFilter(status)}
                    >
                      {status === "all" ? "全部" : statusMeta[status].label} {counts[status]}
                    </button>
                  ))}
                  <span className="mx-1 hidden h-5 w-px bg-border sm:block" />
                  <select
                    className="h-7 rounded-md border border-input bg-transparent px-2 text-xs text-foreground/80 outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
                    value={kindFilter}
                    onChange={(event) => setKindFilter(event.target.value as KindFilter)}
                  >
                    <option value="all">全部类型</option>
                    <option value="note">笔记</option>
                    <option value="link">链接</option>
                    <option value="github">GitHub</option>
                  </select>
                  <select
                    className="h-7 rounded-md border border-input bg-transparent px-2 text-xs text-foreground/80 outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
                    value={sortOrder}
                    onChange={(event) => setSortOrder(event.target.value as ItemSort)}
                  >
                    <option value="updated">最近更新</option>
                    <option value="created">最近创建</option>
                  </select>
                </div>
              </header>

              <div className="grid min-h-0 flex-1 lg:grid-cols-[minmax(18rem,0.9fr)_minmax(22rem,1.1fr)]">
                <section
                  className="min-h-72 border-b border-border bg-background p-4 sm:p-5 lg:border-r lg:border-b-0"
                  aria-label="Inbox 列表"
                  aria-busy={initialLoading}
                >
                  {initialLoading ? (
                    <div className="space-y-2" role="status" aria-label="正在加载">
                      {Array.from({ length: 5 }).map((_, index) => (
                        <div
                          key={index}
                          className="rounded-lg border border-border bg-card p-4"
                          aria-hidden
                        >
                          <div className="h-4 w-3/5 animate-pulse rounded bg-muted" />
                          <div className="mt-2 h-3 w-full animate-pulse rounded bg-muted/70" />
                          <div className="mt-3 h-3 w-1/3 animate-pulse rounded bg-muted/70" />
                        </div>
                      ))}
                    </div>
                  ) : null}
                  {!initialLoading && error ? (
                    <div className="rounded-lg border border-destructive/30 bg-destructive/10 p-5 text-sm text-destructive">
                      <p>{error}</p>
                      <button
                        className="mt-3 font-medium underline"
                        type="button"
                        onClick={() => void loadFirstPage()}
                      >
                        重新加载
                      </button>
                    </div>
                  ) : null}
                  {!initialLoading && !error && visibleItems.length === 0 ? (
                    <div className="rounded-lg border border-dashed border-border bg-card p-8 text-center">
                      <p className="text-sm font-medium text-foreground/80">
                        {searching ? "没有匹配的内容" : "Inbox 还是空的"}
                      </p>
                      <p className="mt-2 text-sm leading-6 text-muted-foreground">
                        {searching
                          ? "换一个关键词，或清除搜索条件。"
                          : "按 Ctrl + Shift + Space 开始快速收集。"}
                      </p>
                    </div>
                  ) : null}
                  <div className="space-y-2">
                    {visibleItems.map((item) => {
                      const active = item.id === selectedId;
                      return (
                        <button
                          key={item.id}
                          className={`block w-full rounded-lg border p-4 text-left transition ${active ? "border-link/60 bg-card" : "border-border bg-card hover:border-foreground/30"}`}
                          type="button"
                          aria-current={active ? "true" : undefined}
                          onClick={() => setSelectedId(item.id)}
                        >
                          <div className="flex items-start justify-between gap-3">
                            <span className="line-clamp-2 min-w-0 text-sm font-medium leading-5">
                              {getItemLabel(item)}
                            </span>
                            <span
                              className={`shrink-0 rounded px-1.5 py-0.5 font-mono text-[10px] uppercase tracking-wider ${kindMeta[item.kind].tone}`}
                            >
                              {kindMeta[item.kind].label}
                            </span>
                          </div>
                          <p className="mt-2 line-clamp-2 text-xs leading-5 text-muted-foreground">
                            {describeItem(item)}
                          </p>
                          <div className="mt-3 flex items-center justify-between gap-3">
                            <span
                              className={`rounded px-1.5 py-0.5 font-mono text-[10px] uppercase tracking-wider ${statusMeta[item.status].tone}`}
                            >
                              {statusMeta[item.status].label}
                            </span>
                            <time
                              className="font-mono text-[11px] text-muted-foreground"
                              dateTime={item.updatedAt}
                            >
                              {formatDate(item.updatedAt)}
                            </time>
                          </div>
                        </button>
                      );
                    })}
                  </div>

                  {!initialLoading && !error ? (
                    <div ref={sentinelRef} className="mt-3 flex justify-center pb-2">
                      {searching ? (
                        <span className="font-mono text-[11px] text-muted-foreground">
                          搜索结果最多 {SEARCH_LIMIT} 条
                        </span>
                      ) : loadingMore ? (
                        <span className="font-mono text-[11px] text-muted-foreground">
                          正在加载更多…
                        </span>
                      ) : hasMore ? (
                        <button
                          className="h-8 rounded-lg border border-border px-3 text-xs text-foreground/80 transition hover:bg-muted"
                          type="button"
                          onClick={() => void loadMore()}
                        >
                          加载更多
                        </button>
                      ) : visibleItems.length > 0 ? (
                        <span className="font-mono text-[11px] text-muted-foreground">
                          已显示全部
                        </span>
                      ) : null}
                    </div>
                  ) : null}
                </section>

                <section className="min-w-0 bg-card p-5 sm:p-7" aria-label="Item 详情">
                  {selectedItem ? (
                    <div className="mx-auto max-w-2xl">
                      <div className="flex flex-wrap items-start justify-between gap-4">
                        <div className="min-w-0">
                          <div className="flex flex-wrap items-center gap-2">
                            <span
                              className={`rounded px-1.5 py-0.5 font-mono text-[10px] uppercase tracking-wider ${kindMeta[selectedItem.kind].tone}`}
                            >
                              {kindMeta[selectedItem.kind].label}
                            </span>
                            <span
                              className={`rounded px-1.5 py-0.5 font-mono text-[10px] uppercase tracking-wider ${statusMeta[selectedItem.status].tone}`}
                            >
                              {statusMeta[selectedItem.status].label}
                            </span>
                          </div>
                          <h3 className="mt-4 break-words text-2xl font-bold leading-tight tracking-tight">
                            {getItemLabel(selectedItem)}
                          </h3>
                          <p className="mt-2 font-mono text-xs text-muted-foreground">
                            更新于 {formatDate(selectedItem.updatedAt)} · 创建于{" "}
                            {formatDate(selectedItem.createdAt)}
                          </p>
                        </div>
                        {selectedItem.url ? (
                          <button
                            className="h-8 shrink-0 rounded-lg bg-primary px-3 text-sm font-medium text-primary-foreground transition hover:bg-primary/90 active:translate-y-px"
                            type="button"
                            onClick={() => void openSelectedLink()}
                          >
                            打开原链接
                          </button>
                        ) : null}
                      </div>

                      <div className="mt-7 space-y-5">
                        <div>
                          <p className="font-mono text-[10px] uppercase tracking-wider text-muted-foreground">
                            正文
                          </p>
                          {bodyLoading ? (
                            <div className="mt-2 space-y-2" role="status" aria-label="正在加载正文">
                              {Array.from({ length: 3 }).map((_, index) => (
                                <div
                                  key={index}
                                  className="h-3 animate-pulse rounded bg-muted"
                                  style={{ width: `${92 - index * 14}%` }}
                                />
                              ))}
                            </div>
                          ) : bodyError ? (
                            <p className="mt-2 text-sm text-destructive">{bodyError}</p>
                          ) : detailBody?.body?.trim() ? (
                            <p className="mt-2 whitespace-pre-wrap text-sm leading-7 text-foreground/80">
                              {detailBody.body}
                            </p>
                          ) : (
                            <p className="mt-2 text-sm text-muted-foreground">
                              {selectedItem.kind === "link"
                                ? "没有可显示的正文，可打开原链接查看。"
                                : "这条内容没有正文。"}
                            </p>
                          )}
                        </div>

                        {selectedItem.url ? (
                          <div>
                            <p className="font-mono text-[10px] uppercase tracking-wider text-muted-foreground">
                              URL
                            </p>
                            <p className="mt-2 break-all rounded-md bg-background px-3 py-2 font-mono text-xs text-foreground/80">
                              {selectedItem.url}
                            </p>
                          </div>
                        ) : null}
                        {selectedItem.summary ? (
                          <div>
                            <p className="font-mono text-[10px] uppercase tracking-wider text-muted-foreground">
                              摘要
                            </p>
                            <p className="mt-2 whitespace-pre-wrap text-sm leading-7 text-foreground/80">
                              {selectedItem.summary}
                            </p>
                          </div>
                        ) : null}
                        {selectedItem.note ? (
                          <div>
                            <p className="font-mono text-[10px] uppercase tracking-wider text-muted-foreground">
                              备注
                            </p>
                            <p className="mt-2 whitespace-pre-wrap rounded-md bg-background px-3 py-3 text-sm leading-7 text-foreground/80">
                              {selectedItem.note}
                            </p>
                          </div>
                        ) : null}
                        <div>
                          <p className="font-mono text-[10px] uppercase tracking-wider text-muted-foreground">
                            来源
                          </p>
                          <p className="mt-2 text-sm text-foreground/80">{selectedItem.source}</p>
                        </div>
                      </div>

                      <div className="mt-8 border-t border-border pt-6">
                        <p className="text-sm font-medium">处理状态</p>
                        <div className="mt-3 flex flex-wrap gap-2">
                          {(["pending", "processed", "archived"] as const).map((status) => (
                            <button
                              key={status}
                              className={`h-8 rounded-lg border px-3 text-sm font-medium transition ${selectedItem.status === status ? "border-primary bg-primary text-primary-foreground" : "border-border text-foreground/80 hover:border-foreground/30"}`}
                              type="button"
                              disabled={mutating}
                              onClick={() => void changeStatus(status)}
                            >
                              {statusMeta[status].label}
                            </button>
                          ))}
                        </div>
                      </div>

                      <div className="mt-8 border-t border-border pt-6">
                        <div className="flex items-center justify-between gap-3">
                          <p className="text-sm font-medium">标签</p>
                          <span className="font-mono text-[11px] text-muted-foreground">
                            点击切换
                          </span>
                        </div>
                        <div className="mt-3 flex flex-wrap gap-2">
                          {tags.map((tag) => {
                            const active = selectedTags.some(
                              (selectedTag) => selectedTag.id === tag.id,
                            );
                            return (
                              <button
                                key={tag.id}
                                className={`h-7 rounded-md border px-2.5 font-mono text-xs transition ${active ? "border-link/60 bg-accent text-link" : "border-border text-muted-foreground hover:border-foreground/30 hover:text-foreground"}`}
                                type="button"
                                disabled={tagSaving}
                                onClick={() => void toggleTag(tag.id)}
                              >
                                {active ? "✓ " : ""}
                                {tag.name}
                              </button>
                            );
                          })}
                          {tags.length === 0 ? (
                            <span className="text-sm text-muted-foreground">还没有标签</span>
                          ) : null}
                        </div>
                        <form className="mt-4 flex max-w-sm gap-2" onSubmit={addTag}>
                          <input
                            className="h-8 min-w-0 flex-1 rounded-lg border border-input bg-transparent px-2.5 text-sm outline-none placeholder:text-muted-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
                            value={newTagName}
                            onChange={(event) => setNewTagName(event.target.value)}
                            placeholder="新建标签"
                            maxLength={128}
                          />
                          <button
                            className="h-8 rounded-lg border border-border px-3 text-sm font-medium text-foreground/80 transition hover:bg-muted disabled:opacity-50"
                            type="submit"
                            disabled={!newTagName.trim() || tagSaving}
                          >
                            添加
                          </button>
                        </form>
                      </div>
                    </div>
                  ) : (
                    <div className="flex min-h-72 items-center justify-center text-center">
                      <div>
                        <p className="text-sm font-medium text-foreground/80">
                          选择一条内容查看详情
                        </p>
                        <p className="mt-2 text-sm text-muted-foreground">
                          正文只在点开时加载，列表不会携带全文。
                        </p>
                      </div>
                    </div>
                  )}
                </section>
              </div>
            </>
          )}
        </section>
      </div>
    </main>
  );
}

export default Inbox;
