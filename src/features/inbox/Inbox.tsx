import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openUrl } from "@tauri-apps/plugin-opener";
import GithubPanel from "../github/GithubPanel";

import {
  createTag,
  exportItemsJson,
  exportItemsMarkdown,
  getItemTags,
  listItems,
  listTags,
  openCapture,
  searchItems,
  setItemTags,
  updateItem,
  type Item,
  type ItemKind,
  type ItemStatus,
  type Tag,
} from "../../lib/api";
import { appEnvironment } from "../../lib/env";

type StatusFilter = "all" | ItemStatus;
type KindFilter = "all" | ItemKind;
type SortOrder = "updated" | "created";

const statusMeta: Record<ItemStatus, { label: string; tone: string }> = {
  pending: { label: "待处理", tone: "bg-tomato-100 text-tomato-600" },
  processed: { label: "已处理", tone: "bg-emerald-100 text-emerald-700" },
  archived: { label: "已归档", tone: "bg-slate-100 text-slate-600" },
};

const kindMeta: Record<ItemKind, { label: string; tone: string }> = {
  note: { label: "笔记", tone: "bg-amber-100 text-amber-700" },
  link: { label: "链接", tone: "bg-sky-100 text-sky-700" },
  github: { label: "GitHub", tone: "bg-violet-100 text-violet-700" },
};

function formatDate(value: string) {
  return new Intl.DateTimeFormat("zh-CN", {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}

function getItemLabel(item: Item) {
  return item.title?.trim() || item.summary?.trim() || item.note?.trim() || item.source;
}

function describeItem(item: Item) {
  if (item.summary?.trim()) return item.summary.trim();
  if (item.note?.trim()) return item.note.trim();
  return item.url || item.source;
}

function Inbox() {
  const [items, setItems] = useState<Item[]>([]);
  const [tags, setTags] = useState<Tag[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [selectedTags, setSelectedTags] = useState<Tag[]>([]);
  const [query, setQuery] = useState("");
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("pending");
  const [kindFilter, setKindFilter] = useState<KindFilter>("all");
  const [sortOrder, setSortOrder] = useState<SortOrder>("updated");
  const [loading, setLoading] = useState(true);
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

  const refresh = useCallback(async (search: string) => {
    const currentRequest = ++requestId.current;
    setLoading(true);
    setError(null);
    try {
      const [nextItems, nextTags] = await Promise.all([
        search ? searchItems(search) : listItems(),
        listTags(),
      ]);
      if (currentRequest !== requestId.current) return;
      setItems(nextItems);
      setTags(nextTags);
      setSelectedId((current) =>
        current && nextItems.some((item) => item.id === current)
          ? current
          : (nextItems[0]?.id ?? null),
      );
    } catch (loadError) {
      if (currentRequest !== requestId.current) return;
      setError(loadError instanceof Error ? loadError.message : "加载 Inbox 失败，请重试。");
    } finally {
      if (currentRequest === requestId.current) setLoading(false);
    }
  }, []);

  const refreshGithubItems = useCallback(() => {
    void refresh(query.trim());
  }, [query, refresh]);

  useEffect(() => {
    const timer = window.setTimeout(() => void refresh(query.trim()), 220);
    return () => window.clearTimeout(timer);
  }, [query, refresh]);

  useEffect(() => {
    let disposed = false;
    const listener = getCurrentWindow().listen("github-sync-updated", () => {
      if (!disposed) refreshGithubItems();
    });
    void listener.then((stop) => {
      if (disposed) stop();
    });
    return () => {
      disposed = true;
      void listener.then((stop) => stop()).catch(() => {});
    };
  }, [refreshGithubItems]);

  useEffect(() => {
    let disposed = false;
    const currentWindow = getCurrentWindow();
    const listener = currentWindow.listen("quick-capture-completed", () => {
      if (!disposed) void refresh(query.trim());
    });
    void listener.then((stop) => {
      if (disposed) stop();
    });
    return () => {
      disposed = true;
      void listener.then((stop) => stop()).catch(() => {});
    };
  }, [query, refresh]);

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

  const selectedItem = items.find((item) => item.id === selectedId) ?? null;

  useEffect(() => {
    if (!selectedItem) {
      setSelectedTags([]);
      return;
    }
    let disposed = false;
    void getItemTags(selectedItem.id)
      .then((nextTags) => {
        if (!disposed) setSelectedTags(nextTags);
      })
      .catch(() => {
        if (!disposed) setSelectedTags([]);
      });
    return () => {
      disposed = true;
    };
  }, [selectedItem]);

  const visibleItems = useMemo(() => {
    return items
      .filter((item) => statusFilter === "all" || item.status === statusFilter)
      .filter((item) => kindFilter === "all" || item.kind === kindFilter)
      .sort((left, right) => {
        const leftValue = sortOrder === "created" ? left.createdAt : left.updatedAt;
        const rightValue = sortOrder === "created" ? right.createdAt : right.updatedAt;
        return rightValue.localeCompare(leftValue);
      });
  }, [items, kindFilter, sortOrder, statusFilter]);

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
      setItems((current) => current.map((item) => (item.id === updated.id ? updated : item)));
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

  const statusCounts = useMemo(
    () => ({
      all: items.length,
      pending: items.filter((item) => item.status === "pending").length,
      processed: items.filter((item) => item.status === "processed").length,
      archived: items.filter((item) => item.status === "archived").length,
    }),
    [items],
  );

  return (
    <main className="min-h-screen bg-canvas text-ink-950">
      <div className="mx-auto flex min-h-screen max-w-[1500px] flex-col lg:flex-row">
        <aside className="flex w-full shrink-0 flex-col border-b border-line bg-surface px-5 py-5 lg:w-64 lg:border-r lg:border-b-0 lg:px-6 lg:py-7">
          <div className="flex items-center justify-between gap-4 lg:block">
            <div>
              <p className="text-xs font-semibold uppercase tracking-[0.18em] text-tomato-600">
                TomatoDesk
              </p>
              <h1 className="mt-1 text-xl font-semibold tracking-tight">个人收件箱</h1>
            </div>
            <span className="rounded-full border border-line px-2.5 py-1 text-xs text-ink-500">
              {appEnvironment}
            </span>
          </div>

          <button
            className="mt-6 flex items-center justify-center rounded-md bg-tomato-600 px-4 py-2.5 text-sm font-semibold text-white transition hover:bg-tomato-500 focus:outline-2 focus:outline-offset-2 focus:outline-tomato-600"
            type="button"
            onClick={() => void openCapture()}
          >
            快速收集
          </button>

          <nav className="mt-7 grid grid-cols-2 gap-2 lg:grid-cols-1" aria-label="主导航">
            <button
              className={`flex items-center justify-between rounded-md px-3 py-2.5 text-left text-sm font-semibold ${showGithub ? "text-ink-500" : "bg-tomato-50 text-tomato-600"}`}
              type="button"
              onClick={() => setShowGithub(false)}
            >
              <span>Inbox</span>
              <span className="rounded-full bg-tomato-100 px-2 py-0.5 text-xs">
                {statusCounts.pending}
              </span>
            </button>
            <button
              className={`flex items-center justify-between rounded-md px-3 py-2.5 text-left text-sm ${showGithub ? "bg-tomato-50 font-semibold text-tomato-600" : "text-ink-500 hover:bg-canvas hover:text-ink-950"}`}
              type="button"
              onClick={() => setShowGithub(true)}
            >
              <span>GitHub</span>
              <span className="text-xs">同步</span>
            </button>
          </nav>

          <section className="mt-7 border-t border-line pt-5" aria-label="导出数据">
            <h2 className="text-sm font-semibold">导出数据</h2>
            <div className="mt-3 grid grid-cols-2 gap-2 lg:grid-cols-1">
              <button
                className="rounded-md border border-line px-3 py-2 text-sm font-medium text-ink-700 hover:bg-canvas disabled:opacity-50"
                type="button"
                disabled={exporting}
                onClick={() => void exportData("json")}
              >
                导出 JSON
              </button>
              <button
                className="rounded-md border border-line px-3 py-2 text-sm font-medium text-ink-700 hover:bg-canvas disabled:opacity-50"
                type="button"
                disabled={exporting}
                onClick={() => void exportData("markdown")}
              >
                导出 Markdown
              </button>
            </div>
            {exportMessage ? (
              <p
                className={`mt-2 break-words text-xs leading-5 ${exportFailed ? "text-red-700" : "text-emerald-700"}`}
                role={exportFailed ? "alert" : "status"}
              >
                {exportMessage}
              </p>
            ) : null}
          </section>

          <div className="mt-auto hidden border-t border-line pt-5 text-xs leading-5 text-ink-500 lg:block">
            <p>Ctrl + K 聚焦搜索</p>
            <p>数据保存在本机 SQLite</p>
          </div>
        </aside>

        <section className="flex min-w-0 flex-1 flex-col">
          {showGithub ? (
            <GithubPanel onSynced={refreshGithubItems} />
          ) : (
            <>
              <header className="border-b border-line bg-surface px-5 py-5 sm:px-7">
                <div className="flex flex-col gap-4 xl:flex-row xl:items-center xl:justify-between">
                  <div>
                    <p className="text-sm font-medium text-ink-500">Inbox</p>
                    <h2 className="mt-1 text-2xl font-semibold tracking-tight">找到要处理的内容</h2>
                  </div>
                  <div className="flex min-w-0 flex-1 gap-3 xl:max-w-2xl">
                    <label className="relative min-w-0 flex-1">
                      <span className="sr-only">搜索标题、备注和来源</span>
                      <input
                        ref={searchRef}
                        className="w-full rounded-md border border-line bg-canvas px-3 py-2.5 pr-16 text-sm outline-none transition placeholder:text-ink-500 focus:border-tomato-600 focus:ring-2 focus:ring-tomato-100"
                        type="search"
                        value={query}
                        placeholder="搜索标题、备注和来源"
                        onChange={(event) => setQuery(event.target.value)}
                      />
                      <kbd className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 rounded border border-line bg-surface px-1.5 py-0.5 text-[11px] text-ink-500">
                        Ctrl K
                      </kbd>
                    </label>
                    <button
                      className="shrink-0 rounded-md border border-line px-3 py-2 text-sm text-ink-700 hover:bg-canvas"
                      type="button"
                      onClick={() => void refresh(query.trim())}
                    >
                      刷新
                    </button>
                  </div>
                </div>

                <div className="mt-5 flex flex-wrap items-center gap-2">
                  {(["all", "pending", "processed", "archived"] as const).map((status) => (
                    <button
                      key={status}
                      className={`rounded-full px-3 py-1.5 text-xs font-semibold transition ${statusFilter === status ? "bg-ink-950 text-white" : "bg-canvas text-ink-500 hover:text-ink-950"}`}
                      type="button"
                      onClick={() => setStatusFilter(status)}
                    >
                      {status === "all" ? "全部" : statusMeta[status].label} {statusCounts[status]}
                    </button>
                  ))}
                  <span className="mx-1 hidden h-5 w-px bg-line sm:block" />
                  <select
                    className="rounded-md border border-line bg-surface px-2.5 py-1.5 text-xs text-ink-700 outline-none focus:border-tomato-600"
                    value={kindFilter}
                    onChange={(event) => setKindFilter(event.target.value as KindFilter)}
                  >
                    <option value="all">全部类型</option>
                    <option value="note">笔记</option>
                    <option value="link">链接</option>
                    <option value="github">GitHub</option>
                  </select>
                  <select
                    className="rounded-md border border-line bg-surface px-2.5 py-1.5 text-xs text-ink-700 outline-none focus:border-tomato-600"
                    value={sortOrder}
                    onChange={(event) => setSortOrder(event.target.value as SortOrder)}
                  >
                    <option value="updated">最近更新</option>
                    <option value="created">最近创建</option>
                  </select>
                </div>
              </header>

              <div className="grid min-h-0 flex-1 lg:grid-cols-[minmax(18rem,0.9fr)_minmax(22rem,1.1fr)]">
                <section
                  className="min-h-72 border-b border-line bg-canvas p-4 sm:p-5 lg:border-r lg:border-b-0"
                  aria-label="Inbox 列表"
                >
                  {loading ? (
                    <p className="rounded-md border border-dashed border-line bg-surface p-6 text-sm text-ink-500">
                      正在加载 Inbox…
                    </p>
                  ) : null}
                  {!loading && error ? (
                    <div className="rounded-md border border-red-200 bg-red-50 p-5 text-sm text-red-700">
                      <p>{error}</p>
                      <button
                        className="mt-3 font-semibold underline"
                        type="button"
                        onClick={() => void refresh(query.trim())}
                      >
                        重新加载
                      </button>
                    </div>
                  ) : null}
                  {!loading && !error && visibleItems.length === 0 ? (
                    <div className="rounded-md border border-dashed border-line bg-surface p-8 text-center">
                      <p className="text-sm font-semibold text-ink-700">
                        {query ? "没有匹配的内容" : "Inbox 还是空的"}
                      </p>
                      <p className="mt-2 text-sm leading-6 text-ink-500">
                        {query
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
                          className={`block w-full rounded-md border p-4 text-left transition ${active ? "border-tomato-500 bg-white shadow-sm" : "border-line bg-surface hover:border-ink-500"}`}
                          type="button"
                          onClick={() => setSelectedId(item.id)}
                        >
                          <div className="flex items-start justify-between gap-3">
                            <span className="line-clamp-2 min-w-0 text-sm font-semibold leading-5">
                              {getItemLabel(item)}
                            </span>
                            <span
                              className={`shrink-0 rounded-full px-2 py-0.5 text-[11px] font-semibold ${kindMeta[item.kind].tone}`}
                            >
                              {kindMeta[item.kind].label}
                            </span>
                          </div>
                          <p className="mt-2 line-clamp-2 text-xs leading-5 text-ink-500">
                            {describeItem(item)}
                          </p>
                          <div className="mt-3 flex items-center justify-between gap-3 text-[11px] text-ink-500">
                            <span
                              className={`rounded-full px-2 py-0.5 font-semibold ${statusMeta[item.status].tone}`}
                            >
                              {statusMeta[item.status].label}
                            </span>
                            <time dateTime={item.updatedAt}>{formatDate(item.updatedAt)}</time>
                          </div>
                        </button>
                      );
                    })}
                  </div>
                </section>

                <section className="min-w-0 bg-surface p-5 sm:p-7" aria-label="Item 详情">
                  {selectedItem ? (
                    <div className="mx-auto max-w-2xl">
                      <div className="flex flex-wrap items-start justify-between gap-4">
                        <div className="min-w-0">
                          <div className="flex flex-wrap items-center gap-2">
                            <span
                              className={`rounded-full px-2.5 py-1 text-xs font-semibold ${kindMeta[selectedItem.kind].tone}`}
                            >
                              {kindMeta[selectedItem.kind].label}
                            </span>
                            <span
                              className={`rounded-full px-2.5 py-1 text-xs font-semibold ${statusMeta[selectedItem.status].tone}`}
                            >
                              {statusMeta[selectedItem.status].label}
                            </span>
                          </div>
                          <h3 className="mt-4 break-words text-2xl font-semibold leading-tight">
                            {getItemLabel(selectedItem)}
                          </h3>
                          <p className="mt-2 text-xs text-ink-500">
                            更新于 {formatDate(selectedItem.updatedAt)} · 创建于{" "}
                            {formatDate(selectedItem.createdAt)}
                          </p>
                        </div>
                        {selectedItem.url ? (
                          <button
                            className="shrink-0 rounded-md bg-ink-950 px-3 py-2 text-sm font-semibold text-white hover:bg-ink-700"
                            type="button"
                            onClick={() => void openSelectedLink()}
                          >
                            打开原链接
                          </button>
                        ) : null}
                      </div>

                      <div className="mt-7 space-y-5">
                        {selectedItem.url ? (
                          <div>
                            <p className="text-xs font-semibold uppercase tracking-wide text-ink-500">
                              URL
                            </p>
                            <p className="mt-2 break-all rounded-md bg-canvas px-3 py-2.5 text-sm text-ink-700">
                              {selectedItem.url}
                            </p>
                          </div>
                        ) : null}
                        {selectedItem.summary ? (
                          <div>
                            <p className="text-xs font-semibold uppercase tracking-wide text-ink-500">
                              摘要
                            </p>
                            <p className="mt-2 whitespace-pre-wrap text-sm leading-7 text-ink-700">
                              {selectedItem.summary}
                            </p>
                          </div>
                        ) : null}
                        {selectedItem.note ? (
                          <div>
                            <p className="text-xs font-semibold uppercase tracking-wide text-ink-500">
                              备注
                            </p>
                            <p className="mt-2 whitespace-pre-wrap rounded-md bg-canvas px-3 py-3 text-sm leading-7 text-ink-700">
                              {selectedItem.note}
                            </p>
                          </div>
                        ) : null}
                        <div>
                          <p className="text-xs font-semibold uppercase tracking-wide text-ink-500">
                            来源
                          </p>
                          <p className="mt-2 text-sm text-ink-700">{selectedItem.source}</p>
                        </div>
                      </div>

                      <div className="mt-8 border-t border-line pt-6">
                        <p className="text-sm font-semibold">处理状态</p>
                        <div className="mt-3 flex flex-wrap gap-2">
                          {(["pending", "processed", "archived"] as const).map((status) => (
                            <button
                              key={status}
                              className={`rounded-md border px-3 py-2 text-sm font-semibold transition ${selectedItem.status === status ? "border-ink-950 bg-ink-950 text-white" : "border-line text-ink-700 hover:border-ink-500"}`}
                              type="button"
                              disabled={mutating}
                              onClick={() => void changeStatus(status)}
                            >
                              {statusMeta[status].label}
                            </button>
                          ))}
                        </div>
                      </div>

                      <div className="mt-8 border-t border-line pt-6">
                        <div className="flex items-center justify-between gap-3">
                          <p className="text-sm font-semibold">标签</p>
                          <span className="text-xs text-ink-500">点击切换</span>
                        </div>
                        <div className="mt-3 flex flex-wrap gap-2">
                          {tags.map((tag) => {
                            const active = selectedTags.some(
                              (selectedTag) => selectedTag.id === tag.id,
                            );
                            return (
                              <button
                                key={tag.id}
                                className={`rounded-full border px-3 py-1.5 text-xs font-semibold transition ${active ? "border-tomato-600 bg-tomato-50 text-tomato-600" : "border-line text-ink-500 hover:border-ink-500 hover:text-ink-950"}`}
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
                            <span className="text-sm text-ink-500">还没有标签</span>
                          ) : null}
                        </div>
                        <form className="mt-4 flex max-w-sm gap-2" onSubmit={addTag}>
                          <input
                            className="min-w-0 flex-1 rounded-md border border-line bg-canvas px-3 py-2 text-sm outline-none focus:border-tomato-600 focus:ring-2 focus:ring-tomato-100"
                            value={newTagName}
                            onChange={(event) => setNewTagName(event.target.value)}
                            placeholder="新建标签"
                            maxLength={128}
                          />
                          <button
                            className="rounded-md border border-line px-3 py-2 text-sm font-semibold text-ink-700 hover:bg-canvas disabled:opacity-50"
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
                        <p className="text-sm font-semibold text-ink-700">选择一条内容查看详情</p>
                        <p className="mt-2 text-sm text-ink-500">
                          列表中的标题、摘要、备注和标签会显示在这里。
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
