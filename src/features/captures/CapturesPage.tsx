import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openUrl } from "@tauri-apps/plugin-opener";

import { Button, ListItem, ListItemMeta, ListItemTitle, Tabs } from "../../components/ui";
import {
  archiveCapture,
  countCaptures,
  createTag,
  getCaptureBody,
  getCaptureTags,
  listCapturesPage,
  listTags,
  restoreCapture,
  searchCaptures,
  setCaptureTags,
  type CaptureBody,
  type CaptureCounts,
  type CaptureKind,
  type CaptureSort,
  type CaptureSummary,
  type Tag,
} from "../../lib/api";
import { notifyCapturesChanged } from "../../lib/events";
import { formatDate } from "../../lib/format";
import CaptureDetail from "./CaptureDetail";
import { describeCapture, getCaptureLabel, kindLabels } from "./shared";

type ArchivedFilter = "active" | "archived" | "all";
type KindFilter = "all" | CaptureKind;

const PAGE_SIZE = 50;
const SEARCH_LIMIT = 100;

const emptyCounts: CaptureCounts = { all: 0, active: 0, archived: 0 };

/** 随手记：长期保留的个人资产，默认只看未归档，归档只是收起来而不是删除。 */
function CapturesPage() {
  const [captures, setCaptures] = useState<CaptureSummary[]>([]);
  const [tags, setTags] = useState<Tag[]>([]);
  const [counts, setCounts] = useState<CaptureCounts>(emptyCounts);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [selectedTags, setSelectedTags] = useState<Tag[]>([]);
  const [detailBody, setDetailBody] = useState<CaptureBody | null>(null);
  const [bodyLoading, setBodyLoading] = useState(false);
  const [bodyError, setBodyError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [archivedFilter, setArchivedFilter] = useState<ArchivedFilter>("active");
  const [kindFilter, setKindFilter] = useState<KindFilter>("all");
  const [sortOrder, setSortOrder] = useState<CaptureSort>("updated");
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
      archived: archivedFilter === "all" ? null : archivedFilter === "archived",
      kind: kindFilter === "all" ? null : kindFilter,
      sort: sortOrder,
    }),
    [archivedFilter, kindFilter, sortOrder],
  );

  const refreshCounts = useCallback(async (kind: KindFilter) => {
    try {
      setCounts(await countCaptures(kind === "all" ? undefined : kind));
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
        countCaptures(kindFilter === "all" ? undefined : kindFilter),
      ]);
      if (currentRequest !== requestId.current) return;
      setTags(nextTags);
      setCounts(nextCounts);

      if (keyword) {
        const results = await searchCaptures(keyword);
        if (currentRequest !== requestId.current) return;
        setCaptures(results.slice(0, SEARCH_LIMIT));
        setCursor(null);
        setHasMore(false);
      } else {
        const page = await listCapturesPage({ ...filterInput, limit: PAGE_SIZE });
        if (currentRequest !== requestId.current) return;
        setCaptures(page.captures);
        setCursor(page.nextCursor);
        setHasMore(page.hasMore);
      }
    } catch (loadError) {
      if (currentRequest !== requestId.current) return;
      setCaptures([]);
      setCursor(null);
      setHasMore(false);
      setError(loadError instanceof Error ? loadError.message : "加载随手记失败，请重试。");
    } finally {
      if (currentRequest === requestId.current) setInitialLoading(false);
    }
  }, [filterInput, kindFilter, query]);

  const loadMore = useCallback(async () => {
    if (!hasMore || loadingMore || initialLoading || searching || !cursor) return;
    const currentRequest = requestId.current;
    setLoadingMore(true);
    try {
      const page = await listCapturesPage({ ...filterInput, cursor, limit: PAGE_SIZE });
      if (currentRequest !== requestId.current) return;
      setCaptures((current) => {
        const seen = new Set(current.map((capture) => capture.id));
        return [...current, ...page.captures.filter((capture) => !seen.has(capture.id))];
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

  const selectedCapture = captures.find((capture) => capture.id === selectedId) ?? null;

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
    void getCaptureBody(selectedId)
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
    void getCaptureTags(selectedId)
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

  const visibleCaptures = useMemo(() => {
    // 分页与排序都在 SQL 里完成；只有搜索结果（≤100 条）在前端补筛。
    if (!searching) return captures;
    return captures
      .filter((capture) => {
        if (archivedFilter === "all") return true;
        return Boolean(capture.archivedAt) === (archivedFilter === "archived");
      })
      .filter((capture) => kindFilter === "all" || capture.kind === kindFilter);
  }, [archivedFilter, captures, kindFilter, searching]);

  useEffect(() => {
    if (visibleCaptures.length === 0) {
      if (selectedId !== null) setSelectedId(null);
      return;
    }
    if (!visibleCaptures.some((capture) => capture.id === selectedId)) {
      setSelectedId(visibleCaptures[0].id);
    }
  }, [selectedId, visibleCaptures]);

  async function toggleArchived() {
    if (!selectedCapture || mutating) return;
    setMutating(true);
    setError(null);
    try {
      const updated = selectedCapture.archivedAt
        ? await restoreCapture(selectedCapture.id)
        : await archiveCapture(selectedCapture.id);
      // 筛选在后端完成，归档后不再匹配当前筛选的行要就地移除。
      const stillMatches =
        archivedFilter === "all" || Boolean(updated.archivedAt) === (archivedFilter === "archived");
      setCaptures((current) =>
        stillMatches
          ? current.map((capture) =>
              capture.id === updated.id ? { ...capture, ...updated } : capture,
            )
          : current.filter((capture) => capture.id !== updated.id),
      );
      void refreshCounts(kindFilter);
      notifyCapturesChanged();
    } catch (updateError) {
      setError(updateError instanceof Error ? updateError.message : "更新归档状态失败，请重试。");
    } finally {
      setMutating(false);
    }
  }

  async function toggleTag(tagId: string) {
    if (!selectedCapture || tagSaving) return;
    const currentIds = new Set(selectedTags.map((tag) => tag.id));
    if (currentIds.has(tagId)) currentIds.delete(tagId);
    else currentIds.add(tagId);
    setTagSaving(true);
    setError(null);
    try {
      setSelectedTags(await setCaptureTags(selectedCapture.id, [...currentIds]));
    } catch (tagError) {
      setError(tagError instanceof Error ? tagError.message : "更新标签失败，请重试。");
    } finally {
      setTagSaving(false);
    }
  }

  async function addTag(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const name = newTagName.trim();
    if (!name || !selectedCapture || tagSaving) return;
    setTagSaving(true);
    setError(null);
    try {
      const created = await createTag({ name, color: null });
      const nextTags = [...tags.filter((tag) => tag.id !== created.id), created].sort(
        (left, right) => left.name.localeCompare(right.name, "zh-CN"),
      );
      setTags(nextTags);
      setSelectedTags(
        await setCaptureTags(selectedCapture.id, [
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
    if (!selectedCapture?.url) return;
    try {
      if (isTauri()) await openUrl(selectedCapture.url);
      else window.open(selectedCapture.url, "_blank", "noopener,noreferrer");
    } catch {
      setError("打开链接失败，请复制 URL 后重试。");
    }
  }

  const archivedTabs = useMemo(
    () => [
      { id: "active", label: "未归档", count: counts.active },
      { id: "archived", label: "已归档", count: counts.archived },
      { id: "all", label: "全部", count: counts.all },
    ],
    [counts],
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <header className="px-5 pt-4 sm:px-7">
        <div className="flex flex-wrap items-baseline justify-between gap-3">
          <div className="flex items-baseline gap-3">
            <h2 className="text-lg font-bold tracking-tight">随手记</h2>
            <span className="font-mono text-xs text-muted-foreground">
              未归档 {counts.active} / 共 {counts.all}
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
            items={archivedTabs}
            value={archivedFilter}
            onChange={(id) => setArchivedFilter(id as ArchivedFilter)}
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
                  onChange={(event) => setSortOrder(event.target.value as CaptureSort)}
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
          aria-label="随手记列表"
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
          {!initialLoading && !error && visibleCaptures.length === 0 ? (
            <div className="px-4 py-10 text-center">
              <p className="text-sm text-foreground">
                {searching
                  ? "没有匹配的内容"
                  : archivedFilter === "archived"
                    ? "还没有归档的随手记"
                    : "还没有随手记"}
              </p>
              <p className="mt-1 text-sm text-muted-foreground">
                {searching
                  ? "换一个关键词，或清除搜索条件。"
                  : "按 Ctrl + Shift + Space 随手记下链接或想法。"}
              </p>
            </div>
          ) : null}

          <div className="divide-y divide-border">
            {visibleCaptures.map((capture) => (
              <ListItem
                key={capture.id}
                active={capture.id === selectedId}
                flush
                className="px-4 py-3"
                aria-current={capture.id === selectedId ? "true" : undefined}
                onClick={() => setSelectedId(capture.id)}
              >
                <ListItemTitle>{getCaptureLabel(capture)}</ListItemTitle>
                <p className="line-clamp-1 text-sm text-muted-foreground">
                  {describeCapture(capture)}
                </p>
                <ListItemMeta>
                  <span>{kindLabels[capture.kind]}</span>
                  {capture.archivedAt ? <span>已归档</span> : null}
                  <time className="ml-auto" dateTime={capture.updatedAt}>
                    {formatDate(capture.updatedAt)}
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
              ) : visibleCaptures.length > 0 ? (
                <span className="font-mono text-xs text-muted-foreground">已显示全部</span>
              ) : null}
            </div>
          ) : null}
        </section>

        <CaptureDetail
          capture={selectedCapture}
          body={detailBody}
          bodyLoading={bodyLoading}
          bodyError={bodyError}
          tags={tags}
          selectedTags={selectedTags}
          tagSaving={tagSaving}
          mutating={mutating}
          newTagName={newTagName}
          onNewTagNameChange={setNewTagName}
          onOpenLink={() => void openSelectedLink()}
          onToggleArchived={() => void toggleArchived()}
          onToggleTag={(tagId) => void toggleTag(tagId)}
          onAddTag={addTag}
        />
      </div>
    </div>
  );
}

export default CapturesPage;
