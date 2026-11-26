import { useEffect, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";

import { Button, ListItem, ListItemMeta, ListItemTitle } from "../../components/ui";
import { getCaptureBody, searchCaptures, type CaptureSummary } from "../../lib/api";
import { formatDate } from "../../lib/format";
import { getCaptureLabel, kindLabels } from "../captures/shared";

const SEARCH_LIMIT = 100;

/** 全局搜索：覆盖标题、摘要、备注与正文（FTS5），结果上限 100 条。 */
function SearchPage() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<CaptureSummary[]>([]);
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
      void searchCaptures(keyword)
        .then((captures) => {
          if (current !== requestId.current) return;
          setResults(captures);
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

  const selected = results.find((capture) => capture.id === selectedId) ?? null;

  useEffect(() => {
    if (!selectedId) {
      setBody(null);
      return;
    }
    let disposed = false;
    setBodyLoading(true);
    void getCaptureBody(selectedId)
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
      <header className="border-b border-border px-5 py-4 sm:px-7">
        <div className="flex flex-wrap items-baseline justify-between gap-3">
          <div className="flex items-baseline gap-3">
            <h2 className="text-lg font-bold tracking-tight">搜索</h2>
            <span className="font-mono text-xs text-muted-foreground">
              {query.trim()
                ? `命中 ${results.length} 条${results.length >= SEARCH_LIMIT ? "（已达上限）" : ""}`
                : "标题 / 摘要 / 备注 / 正文"}
            </span>
          </div>
          <label className="relative min-w-0 flex-1 sm:max-w-md">
            <span className="sr-only">搜索标题、正文和来源</span>
            <input
              ref={inputRef}
              className="h-8 w-full rounded-lg border border-input bg-transparent px-2.5 text-sm outline-none transition placeholder:text-muted-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
              type="search"
              value={query}
              placeholder="输入关键词"
              onChange={(event) => setQuery(event.target.value)}
            />
          </label>
        </div>
      </header>

      {error ? (
        <p className="callout callout-error border-b border-border px-5 py-2.5 text-sm text-destructive sm:px-7">
          {error}
        </p>
      ) : null}

      <div className="grid min-h-0 flex-1 lg:grid-cols-[minmax(18rem,0.9fr)_minmax(22rem,1.1fr)]">
        <div className="min-h-0 overflow-y-auto border-b border-border lg:border-r lg:border-b-0">
          {loading ? (
            <p className="px-4 py-3 font-mono text-xs text-muted-foreground">正在搜索…</p>
          ) : null}
          {!loading && query.trim() && results.length === 0 && !error ? (
            <p className="px-4 py-8 text-center text-sm text-muted-foreground">没有匹配的内容。</p>
          ) : null}
          <div className="divide-y divide-border">
            {results.map((capture) => (
              <ListItem
                key={capture.id}
                active={capture.id === selectedId}
                flush
                className="px-4 py-3"
                aria-current={capture.id === selectedId ? "true" : undefined}
                onClick={() => setSelectedId(capture.id)}
              >
                <ListItemTitle>{getCaptureLabel(capture)}</ListItemTitle>
                <ListItemMeta>
                  <span>{kindLabels[capture.kind]}</span>
                  <span>{capture.source}</span>
                  <span className="ml-auto">{formatDate(capture.updatedAt)}</span>
                </ListItemMeta>
              </ListItem>
            ))}
          </div>
        </div>

        <div className="min-w-0 overflow-y-auto bg-card px-5 py-6 sm:px-7">
          {selected ? (
            <div className="mx-auto max-w-2xl">
              <h3 className="text-xl leading-snug font-bold tracking-tight break-words">
                {getCaptureLabel(selected)}
              </h3>
              <p className="mt-1 font-mono text-xs text-muted-foreground">
                {selected.source} · 更新于 {formatDate(selected.updatedAt)}
              </p>
              {selected.url ? (
                <Button
                  variant="outline"
                  size="sm"
                  className="mt-3"
                  onClick={() => selected.url && void openLink(selected.url)}
                >
                  打开链接
                </Button>
              ) : null}
              <div className="mt-5">
                <p className="text-xs font-medium text-muted-foreground">正文</p>
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
                  <p className="mt-2 text-sm leading-7 whitespace-pre-wrap">{body}</p>
                ) : (
                  <p className="mt-2 text-sm text-muted-foreground">这条随手记没有正文。</p>
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
