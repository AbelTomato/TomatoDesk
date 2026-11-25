import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openUrl } from "@tauri-apps/plugin-opener";

import { Button, ListItem, ListItemMeta, ListItemTitle, Tabs, cn } from "../../components/ui";
import {
  countItemsByStatus,
  createTag,
  getItemBody,
  getItemTags,
  listItemsPage,
  listTags,
  searchItems,
  setItemTags,
  toUpdateItemInput,
  updateItem,
  type ItemBody,
  type ItemKind,
  type ItemSort,
  type ItemStatus,
  type ItemStatusCounts,
  type ItemSummary,
  type Tag,
} from "../../lib/api";
import { notifyItemsChanged } from "../../lib/events";
import { formatDate } from "../../lib/format";

type StatusFilter = "all" | ItemStatus;
type KindFilter = "all" | ItemKind;

const PAGE_SIZE = 50;
const SEARCH_LIMIT = 100;

const emptyCounts: ItemStatusCounts = { all: 0, pending: 0, processed: 0, archived: 0 };

const statusLabels: Record<ItemStatus, string> = {
  pending: "待处理",
  processed: "已处理",
  archived: "已归档",
};

const kindLabels: Record<ItemKind, string> = {
  note: "笔记",
  link: "链接",
  github: "GitHub",
};

const statusOrder: ItemStatus[] = ["pending", "processed", "archived"];

function getItemLabel(item: ItemSummary) {
  return item.title?.trim() || item.summary?.trim() || item.note?.trim() || item.source;
}

function describeItem(item: ItemSummary) {
  if (item.summary?.trim()) return item.summary.trim();
  if (item.note?.trim()) return item.note.trim();
  return item.url || item.source;
}

/** 个人收集：只放笔记与链接，GitHub 内容在「仓库」页按仓库组织。 */
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
      const updated = await updateItem(toUpdateItemInput(selectedItem, status));
      // 筛选在后端完成，改完状态后不再匹配当前筛选的行要就地移除。
      const stillMatches = statusFilter === "all" || updated.status === statusFilter;
      setItems((current) =>
        stillMatches
          ? current.map((item) => (item.id === updated.id ? { ...item, ...updated } : item))
          : current.filter((item) => item.id !== updated.id),
      );
      void refreshCounts(kindFilter);
      notifyItemsChanged();
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

  const statusTabs = useMemo(
    () => [
      { id: "all", label: "全部", count: counts.all },
      ...statusOrder.map((status) => ({
        id: status,
        label: statusLabels[status],
        count: counts[status],
      })),
    ],
    [counts],
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <header className="px-5 pt-4 sm:px-7">
        <div className="flex flex-wrap items-baseline justify-between gap-3">
          <div className="flex items-baseline gap-3">
            <h2 className="text-lg font-bold tracking-tight">个人收集</h2>
            <span className="font-mono text-xs text-muted-foreground">
              待处理 {counts.pending} / 共 {counts.all}
            </span>
          </div>
          <div className="flex min-w-0 flex-1 items-center justify-end gap-2 sm:max-w-md">
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
              <kbd className="pointer-events-none absolute top-1/2 right-2 -translate-y-1/2 rounded border border-border px-1.5 py-0.5 font-mono text-xs text-muted-foreground">
                Ctrl K
              </kbd>
            </label>
            <Button variant="ghost" size="sm" onClick={() => void loadFirstPage()}>
              刷新
            </Button>
          </div>
        </div>

        <div className="mt-3">
          <Tabs
            items={statusTabs}
            value={statusFilter}
            onChange={(id) => setStatusFilter(id as StatusFilter)}
            trailing={
              <>
                <select
                  className="h-7 rounded-md bg-transparent px-1.5 font-mono text-xs text-muted-foreground outline-none hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
                  value={kindFilter}
                  onChange={(event) => setKindFilter(event.target.value as KindFilter)}
                >
                  <option value="all">全部类型</option>
                  <option value="note">笔记</option>
                  <option value="link">链接</option>
                </select>
                <span aria-hidden className="h-4 w-px bg-border" />
                <select
                  className="h-7 rounded-md bg-transparent px-1.5 font-mono text-xs text-muted-foreground outline-none hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
                  value={sortOrder}
                  onChange={(event) => setSortOrder(event.target.value as ItemSort)}
                >
                  <option value="updated">最近更新</option>
                  <option value="created">最近创建</option>
                </select>
              </>
            }
          />
        </div>
      </header>

      <div className="grid min-h-0 flex-1 lg:grid-cols-[minmax(18rem,0.9fr)_minmax(22rem,1.1fr)]">
        <section
          className="min-h-72 overflow-y-auto border-b border-border lg:border-r lg:border-b-0"
          aria-label="Inbox 列表"
          aria-busy={initialLoading}
        >
          {initialLoading ? (
            <div className="divide-y divide-border" role="status" aria-label="正在加载">
              {Array.from({ length: 6 }).map((_, index) => (
                <div key={index} className="space-y-2 px-4 py-3" aria-hidden>
                  <div className="h-3.5 w-3/5 animate-pulse rounded bg-muted" />
                  <div className="h-3 w-4/5 animate-pulse rounded bg-muted/70" />
                </div>
              ))}
            </div>
          ) : null}
          {!initialLoading && error ? (
            <div className="m-4 callout callout-error px-4 py-3 text-sm text-destructive">
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
          {!initialLoading && !error && visibleItems.length === 0 ? (
            <div className="px-4 py-10 text-center">
              <p className="text-sm text-foreground">
                {searching ? "没有匹配的内容" : "还没有收集"}
              </p>
              <p className="mt-1 text-sm text-muted-foreground">
                {searching
                  ? "换一个关键词，或清除搜索条件。"
                  : "按 Ctrl + Shift + Space 开始收集。"}
              </p>
            </div>
          ) : null}

          <div className="divide-y divide-border">
            {visibleItems.map((item) => (
              <ListItem
                key={item.id}
                active={item.id === selectedId}
                flush
                className="px-4 py-3"
                aria-current={item.id === selectedId ? "true" : undefined}
                onClick={() => setSelectedId(item.id)}
              >
                <ListItemTitle>{getItemLabel(item)}</ListItemTitle>
                <p className="line-clamp-1 text-sm text-muted-foreground">{describeItem(item)}</p>
                <ListItemMeta>
                  <span>{kindLabels[item.kind]}</span>
                  <span>{statusLabels[item.status]}</span>
                  <time className="ml-auto" dateTime={item.updatedAt}>
                    {formatDate(item.updatedAt)}
                  </time>
                </ListItemMeta>
              </ListItem>
            ))}
          </div>

          {!initialLoading && !error ? (
            <div ref={sentinelRef} className="flex justify-center px-4 py-4">
              {searching ? (
                <span className="font-mono text-xs text-muted-foreground">
                  搜索结果最多 {SEARCH_LIMIT} 条
                </span>
              ) : loadingMore ? (
                <span className="font-mono text-xs text-muted-foreground">正在加载更多…</span>
              ) : hasMore ? (
                <Button variant="outline" size="sm" onClick={() => void loadMore()}>
                  加载更多
                </Button>
              ) : visibleItems.length > 0 ? (
                <span className="font-mono text-xs text-muted-foreground">已显示全部</span>
              ) : null}
            </div>
          ) : null}
        </section>

        <section
          className="min-w-0 overflow-y-auto bg-card px-5 py-6 sm:px-7"
          aria-label="Item 详情"
        >
          {selectedItem ? (
            <div className="mx-auto max-w-2xl">
              <div className="flex flex-wrap items-start justify-between gap-4">
                <div className="min-w-0">
                  <h3 className="text-xl leading-snug font-bold tracking-tight break-words">
                    {getItemLabel(selectedItem)}
                  </h3>
                  <p className="mt-1 font-mono text-xs text-muted-foreground">
                    {kindLabels[selectedItem.kind]} · {statusLabels[selectedItem.status]} · 更新于{" "}
                    {formatDate(selectedItem.updatedAt)} · 创建于{" "}
                    {formatDate(selectedItem.createdAt)}
                  </p>
                </div>
                {selectedItem.url ? (
                  <Button size="sm" onClick={() => void openSelectedLink()}>
                    打开原链接
                  </Button>
                ) : null}
              </div>

              <div className="mt-6 space-y-5">
                <div>
                  <p className="text-xs font-medium text-muted-foreground">正文</p>
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
                    <p className="mt-2 text-sm leading-7 whitespace-pre-wrap">{detailBody.body}</p>
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
                    <p className="text-xs font-medium text-muted-foreground">URL</p>
                    <p className="mt-2 rounded-md bg-background px-3 py-2 font-mono text-xs break-all text-muted-foreground">
                      {selectedItem.url}
                    </p>
                  </div>
                ) : null}
                {selectedItem.summary ? (
                  <div>
                    <p className="text-xs font-medium text-muted-foreground">摘要</p>
                    <p className="mt-2 text-sm leading-7 whitespace-pre-wrap">
                      {selectedItem.summary}
                    </p>
                  </div>
                ) : null}
                {selectedItem.note ? (
                  <div>
                    <p className="text-xs font-medium text-muted-foreground">备注</p>
                    <p className="mt-2 rounded-md bg-background px-3 py-2.5 text-sm leading-7 whitespace-pre-wrap">
                      {selectedItem.note}
                    </p>
                  </div>
                ) : null}
                <div>
                  <p className="text-xs font-medium text-muted-foreground">来源</p>
                  <p className="mt-2 font-mono text-xs">{selectedItem.source}</p>
                </div>
              </div>

              <div className="mt-8 border-t border-border pt-5">
                <div className="flex items-center justify-between gap-3">
                  <p className="text-xs font-medium text-muted-foreground">处理状态</p>
                  {mutating ? (
                    <span className="font-mono text-xs text-muted-foreground">保存中…</span>
                  ) : null}
                </div>
                <div className="mt-2 flex flex-wrap gap-1.5">
                  {statusOrder.map((status) => (
                    <Button
                      key={status}
                      size="sm"
                      variant={selectedItem.status === status ? "default" : "outline"}
                      disabled={mutating}
                      onClick={() => void changeStatus(status)}
                    >
                      {statusLabels[status]}
                    </Button>
                  ))}
                </div>
              </div>

              <div className="mt-6 border-t border-border pt-5">
                <div className="flex items-center justify-between gap-3">
                  <p className="text-xs font-medium text-muted-foreground">标签</p>
                  <span className="font-mono text-xs text-muted-foreground">点击切换</span>
                </div>
                <div className="mt-2 flex flex-wrap gap-1.5">
                  {tags.map((tag) => {
                    const active = selectedTags.some((selectedTag) => selectedTag.id === tag.id);
                    return (
                      <button
                        key={tag.id}
                        className={cn(
                          "h-7 rounded-md border px-2 font-mono text-xs transition-colors",
                          active
                            ? "border-link/50 text-link"
                            : "border-border text-muted-foreground hover:border-foreground/30 hover:text-foreground",
                        )}
                        type="button"
                        disabled={tagSaving}
                        onClick={() => void toggleTag(tag.id)}
                      >
                        {tag.name}
                      </button>
                    );
                  })}
                  {tags.length === 0 ? (
                    <span className="text-sm text-muted-foreground">还没有标签</span>
                  ) : null}
                </div>
                <form className="mt-3 flex max-w-sm gap-2" onSubmit={addTag}>
                  <input
                    className="h-8 min-w-0 flex-1 rounded-lg border border-input bg-transparent px-2.5 text-sm outline-none placeholder:text-muted-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
                    value={newTagName}
                    onChange={(event) => setNewTagName(event.target.value)}
                    placeholder="新建标签"
                    maxLength={128}
                  />
                  <Button
                    variant="outline"
                    size="sm"
                    type="submit"
                    disabled={!newTagName.trim() || tagSaving}
                  >
                    添加
                  </Button>
                </form>
              </div>
            </div>
          ) : (
            <div className="flex min-h-72 items-center justify-center text-center">
              <div>
                <p className="text-sm">选择一条内容查看详情</p>
                <p className="mt-1 text-sm text-muted-foreground">
                  正文只在点开时加载，列表不会携带全文。
                </p>
              </div>
            </div>
          )}
        </section>
      </div>
    </div>
  );
}

export default Inbox;
