import { useEffect, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";

import { getItemBody, searchItems, type ItemSummary } from "../../lib/api";
import { formatDate } from "../../lib/format";

const SEARCH_LIMIT = 100;

function labelOf(item: ItemSummary) {
  return item.title?.trim() || item.summary?.trim() || item.note?.trim() || item.source;
}

/** 全局搜索：覆盖标题、摘要、备注与正文（FTS5），结果上限 100 条。 */
function SearchPage() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<ItemSummary[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [body, setBody] = useState<string | null>(null);
  const [bodyLoading, setBodyLoading] = useState(false);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const requestId = useRef(0);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  useEffect(() => {
    const keyword = query.trim();
    if (!keyword) {
      setResults([]);
      setError(null);
      setLoading(false);
      return;
    }
    const current = ++requestId.current;
    setLoading(true);
    const timer = window.setTimeout(() => {
      void searchItems(keyword)
        .then((items) => {
          if (current !== requestId.current) return;
          setResults(items);
          setError(null);
        })
        .catch((searchError) => {
          if (current !== requestId.current) return;
          setResults([]);
          setError(searchError instanceof Error ? searchError.message : "搜索失败，请重试。");
        })
        .finally(() => {
          if (current === requestId.current) setLoading(false);
        });
    }, 220);
    return () => window.clearTimeout(timer);
  }, [query]);

  const selected = results.find((item) => item.id === selectedId) ?? null;

  useEffect(() => {
    if (!selectedId) {
      setBody(null);
      return;
    }
    let disposed = false;
    setBodyLoading(true);
    void getItemBody(selectedId)
      .then((value) => {
        if (!disposed) setBody(value.body);
      })
      .catch(() => {
        if (!disposed) setBody(null);
      })
      .finally(() => {
        if (!disposed) setBodyLoading(false);
      });
    return () => {
      disposed = true;
    };
  }, [selectedId]);

  async function openLink(url: string) {
    try {
      if (isTauri()) await openUrl(url);
      else window.open(url, "_blank", "noopener,noreferrer");
    } catch {
      setError("打开链接失败，请复制 URL 后重试。");
    }
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <header className="border-b border-border bg-card px-5 py-5 sm:px-7">
        <p className="text-sm text-muted-foreground">Search</p>
        <h2 className="mt-1 text-2xl font-bold tracking-tight">搜索</h2>
        <label className="relative mt-4 block max-w-2xl">
          <span className="sr-only">搜索标题、正文和来源</span>
          <input
            ref={inputRef}
            className="h-8 w-full rounded-lg border border-input bg-transparent px-2.5 text-sm outline-none transition placeholder:text-muted-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
            type="search"
            value={query}
            placeholder="输入关键词，覆盖标题、摘要、备注与正文"
            onChange={(event) => setQuery(event.target.value)}
          />
        </label>
        <p className="mt-2 font-mono text-[11px] text-muted-foreground">
          {query.trim()
            ? `命中 ${results.length} 条${results.length >= SEARCH_LIMIT ? "（已达上限）" : ""}`
            : "输入关键词开始搜索"}
        </p>
      </header>

      {error ? (
        <p className="border-b border-destructive/30 bg-destructive/10 px-5 py-2 text-sm text-destructive sm:px-7">
          {error}
        </p>
      ) : null}

      <div className="grid min-h-0 flex-1 lg:grid-cols-[minmax(18rem,0.9fr)_minmax(22rem,1.1fr)]">
        <div className="border-b border-border p-4 sm:p-5 lg:border-r lg:border-b-0">
          {loading ? (
            <p className="font-mono text-[11px] text-muted-foreground">正在搜索…</p>
          ) : null}
          {!loading && query.trim() && results.length === 0 && !error ? (
            <p className="text-sm text-muted-foreground">没有匹配的内容。</p>
          ) : null}
          <div className="space-y-2">
            {results.map((item) => {
              const active = item.id === selectedId;
              return (
                <button
                  key={item.id}
                  className={`block w-full rounded-lg border p-3 text-left transition ${active ? "border-link/60 bg-card" : "border-border bg-card hover:border-foreground/30"}`}
                  type="button"
                  aria-current={active ? "true" : undefined}
                  onClick={() => setSelectedId(item.id)}
                >
                  <span className="line-clamp-2 text-sm leading-5 font-medium">
                    {labelOf(item)}
                  </span>
                  <div className="mt-2 flex flex-wrap items-center gap-2 font-mono text-[11px] text-muted-foreground">
                    <span>
                      {item.kind === "note" ? "笔记" : item.kind === "link" ? "链接" : "GitHub"}
                    </span>
                    <span>{item.source}</span>
                    <span className="ml-auto">{formatDate(item.updatedAt)}</span>
                  </div>
                </button>
              );
            })}
          </div>
        </div>

        <div className="min-w-0 bg-card p-5 sm:p-6">
          {selected ? (
            <div className="mx-auto max-w-2xl">
              <h3 className="text-xl leading-snug font-bold tracking-tight break-words">
                {labelOf(selected)}
              </h3>
              <p className="mt-2 font-mono text-xs text-muted-foreground">
                {selected.source} · 更新于 {formatDate(selected.updatedAt)}
              </p>
              {selected.url ? (
                <button
                  className="mt-3 h-8 rounded-lg border border-border px-3 text-sm text-foreground/80 transition hover:bg-muted"
                  type="button"
                  onClick={() => selected.url && void openLink(selected.url)}
                >
                  打开链接
                </button>
              ) : null}
              <div className="mt-5">
                <p className="font-mono text-[10px] tracking-wider text-muted-foreground uppercase">
                  正文
                </p>
                {bodyLoading ? (
                  <div className="mt-2 space-y-2" role="status" aria-label="正在加载正文">
                    {Array.from({ length: 3 }).map((_, index) => (
                      <div
                        key={index}
                        className="h-3 animate-pulse rounded bg-muted"
                        style={{ width: `${90 - index * 15}%` }}
                      />
                    ))}
                  </div>
                ) : body?.trim() ? (
                  <p className="mt-2 text-sm leading-7 whitespace-pre-wrap text-foreground/80">
                    {body}
                  </p>
                ) : (
                  <p className="mt-2 text-sm text-muted-foreground">这条内容没有正文。</p>
                )}
              </div>
            </div>
          ) : (
            <div className="flex min-h-48 items-center justify-center text-center text-sm text-muted-foreground">
              选择一条结果查看正文
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export default SearchPage;
