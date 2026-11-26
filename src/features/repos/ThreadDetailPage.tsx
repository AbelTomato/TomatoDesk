import { useCallback, useEffect, useRef, useState } from "react";

import { Button, SectionTitle, StateMark } from "../../components/ui";
import Markdown from "../../components/markdown/Markdown";
import {
  getThread,
  refreshThreadComments,
  type ThreadComment,
  type ThreadDetail,
} from "../../lib/api";
import { formatDate, formatRelative } from "../../lib/format";
import { openExternal } from "../../lib/openExternal";
import { relationLabel, stateLabels, stateTones, threadHeadline } from "./threadMeta";

const COMMENT_PAGE_SIZE = 50;

interface ThreadDetailPageProps {
  threadId: string;
  /** 面包屑上的仓库名；拿不到时退回「返回列表」。 */
  repoLabel: string | null;
  onBack: () => void;
}

/** 元信息表：PR 比 Issue 多一行「合并」。 */
function MetaList({ thread }: { thread: ThreadDetail }) {
  const rows: Array<[string, string]> = [
    ["状态", stateLabels[thread.state]],
    ["与我的关系", relationLabel(thread) ?? "—"],
    ["作者", thread.author ?? "—"],
    ["编号", `#${thread.number}`],
    ["创建", formatDate(thread.createdAt)],
    ["最近更新", formatDate(thread.updatedAt)],
    ["关闭", thread.closedAt ? formatDate(thread.closedAt) : "—"],
  ];
  if (thread.kind === "pull_request") {
    rows.push(["合并", thread.mergedAt ? formatDate(thread.mergedAt) : "—"]);
  }

  return (
    <dl className="divide-y divide-border text-sm">
      {rows.map(([label, value]) => (
        <div key={label} className="flex items-baseline justify-between gap-3 py-1.5">
          <dt className="text-muted-foreground">{label}</dt>
          <dd className="font-mono text-xs">{value}</dd>
        </div>
      ))}
    </dl>
  );
}

/**
 * PR / Issue 的完整详情页。
 * 右侧概览面板只放摘要与关键元信息，正文、变更规模与全部时间点在这里展开。
 */
function ThreadDetailPage({ threadId, repoLabel, onBack }: ThreadDetailPageProps) {
  const [thread, setThread] = useState<ThreadDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setThread(await getThread(threadId));
    } catch (loadError) {
      setThread(null);
      setError(loadError instanceof Error ? loadError.message : "加载详情失败，请重试。");
    } finally {
      setLoading(false);
    }
  }, [threadId]);

  useEffect(() => {
    void load();
  }, [load]);

  const [comments, setComments] = useState<ThreadComment[]>([]);
  const [commentsLoading, setCommentsLoading] = useState(false);
  const [commentsNotice, setCommentsNotice] = useState<string | null>(null);
  const [visibleComments, setVisibleComments] = useState(COMMENT_PAGE_SIZE);
  /** 防止 StrictMode 双执行或连点触发重复请求。 */
  const commentsRequest = useRef(false);

  const refreshComments = useCallback(
    async (force: boolean) => {
      if (!thread || commentsRequest.current) return;
      commentsRequest.current = true;
      setCommentsLoading(true);
      setCommentsNotice(null);
      try {
        const result = await refreshThreadComments(thread.id, force);
        setComments(result.comments);
        setVisibleComments(COMMENT_PAGE_SIZE);
        if (result.error) setCommentsNotice(result.error);
      } catch (loadError) {
        setCommentsNotice(
          loadError instanceof Error ? loadError.message : "加载评论失败，请重试。",
        );
      } finally {
        commentsRequest.current = false;
        setCommentsLoading(false);
      }
    },
    [thread],
  );

  useEffect(() => {
    if (!thread) return;
    setComments(thread.comments);
    setVisibleComments(COMMENT_PAGE_SIZE);
    setCommentsNotice(null);
    if (thread.commentCount === 0) return;
    // 缓存缺失或条数不足时才回源；时间戳比较在后端做。
    if (!thread.commentsSyncedAt || thread.comments.length < thread.commentCount) {
      void refreshComments(false);
    }
  }, [thread, refreshComments]);

  const relation = thread ? relationLabel(thread) : null;
  const showDiff =
    thread?.kind === "pull_request" &&
    (thread.additions !== null || thread.deletions !== null || thread.changedFiles !== null);

  return (
    <div className="flex min-h-0 flex-1 flex-col bg-background">
      <header className="flex shrink-0 items-center gap-3 border-b border-border px-5 py-2 sm:px-7">
        <button
          className="font-mono text-xs text-muted-foreground transition-colors hover:text-foreground"
          type="button"
          onClick={onBack}
        >
          ← {repoLabel ?? "返回列表"}
        </button>
        {thread ? (
          <span className="ml-auto font-mono text-xs text-muted-foreground">
            {threadHeadline(thread)}
          </span>
        ) : null}
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto">
        {loading ? (
          <div
            className="mx-auto max-w-3xl space-y-3 px-5 py-8 sm:px-7"
            role="status"
            aria-label="正在加载详情"
          >
            <div className="h-3 w-24 animate-pulse rounded bg-muted" />
            <div className="h-6 w-2/3 animate-pulse rounded bg-muted" />
            <div className="h-3 w-1/2 animate-pulse rounded bg-muted/70" />
            <div className="h-32 animate-pulse rounded bg-muted/60" />
          </div>
        ) : error ? (
          <div className="mx-auto max-w-3xl px-5 py-8 sm:px-7">
            <div className="callout callout-error px-4 py-3 text-sm text-destructive">
              <p>{error}</p>
              <button
                className="mt-2 text-xs font-medium underline"
                type="button"
                onClick={() => void load()}
              >
                重新加载
              </button>
            </div>
          </div>
        ) : thread ? (
          <article className="mx-auto max-w-3xl px-5 py-7 sm:px-7">
            <div className="flex flex-wrap items-center gap-3 font-mono text-xs text-muted-foreground">
              <span>{threadHeadline(thread)}</span>
              <StateMark tone={stateTones[thread.state]}>{stateLabels[thread.state]}</StateMark>
              {relation ? <span>{relation}</span> : null}
              <span className="ml-auto">{thread.commentCount} comments</span>
            </div>

            <h2 className="mt-2 text-xl leading-snug font-bold tracking-tight break-words">
              {thread.title}
            </h2>
            <p className="mt-1.5 font-mono text-xs text-muted-foreground">
              {thread.author ?? "未知作者"} · 创建于 {formatDate(thread.createdAt)} · 更新于{" "}
              {formatRelative(thread.updatedAt)}
            </p>

            <div className="mt-3 flex flex-wrap items-center gap-3">
              <Button variant="outline" size="sm" onClick={() => void openExternal(thread.url)}>
                在 GitHub 打开
              </Button>
              {showDiff ? (
                <span className="font-mono text-xs text-muted-foreground">
                  +{thread.additions ?? 0} −{thread.deletions ?? 0}
                  {thread.changedFiles !== null ? ` · ${thread.changedFiles} files changed` : ""}
                </span>
              ) : null}
            </div>

            <section className="mt-8">
              <SectionTitle>正文</SectionTitle>
              {thread.body?.trim() ? (
                <Markdown className="mt-3" source={thread.body} baseUrl={thread.url} />
              ) : (
                <p className="mt-3 text-sm text-muted-foreground">这条内容没有正文。</p>
              )}
            </section>

            <section id="comments" className="mt-8">
              <div className="flex items-baseline justify-between gap-3 border-b border-border pb-2">
                <h3 className="text-sm font-bold tracking-tight">评论</h3>
                <div className="flex items-center gap-3 font-mono text-xs text-muted-foreground">
                  <span>{thread.commentCount} comments</span>
                  <button
                    className="transition-colors hover:text-foreground hover:underline"
                    type="button"
                    disabled={commentsLoading}
                    onClick={() => void refreshComments(true)}
                  >
                    {commentsLoading ? "刷新中…" : "刷新"}
                  </button>
                </div>
              </div>

              {commentsNotice ? (
                <div className="callout callout-warning mt-3 px-3 py-2 text-sm">
                  <p>评论可能不是最新：{commentsNotice}</p>
                  <button
                    className="mt-1 text-xs font-medium underline"
                    type="button"
                    disabled={commentsLoading}
                    onClick={() => void refreshComments(true)}
                  >
                    重新加载评论
                  </button>
                </div>
              ) : null}

              {commentsLoading && comments.length === 0 ? (
                <div className="mt-4 space-y-3" role="status" aria-label="正在加载评论">
                  {Array.from({ length: 3 }).map((_, index) => (
                    <div key={index} className="space-y-2" aria-hidden>
                      <div className="h-3 w-32 animate-pulse rounded bg-muted" />
                      <div className="h-3 w-full animate-pulse rounded bg-muted/70" />
                      <div className="h-3 w-2/3 animate-pulse rounded bg-muted/60" />
                    </div>
                  ))}
                </div>
              ) : comments.length === 0 ? (
                <p className="mt-3 text-sm text-muted-foreground">
                  {thread.commentCount > 0
                    ? "评论尚未同步，稍后会自动重试，也可以点「刷新」。"
                    : "这条内容没有评论。"}
                </p>
              ) : (
                <div className="divide-y divide-border">
                  {comments.slice(0, visibleComments).map((comment) => (
                    <article key={comment.id} className="py-4">
                      <div className="flex flex-wrap items-baseline gap-3 font-mono text-xs text-muted-foreground">
                        <span className="text-foreground">{comment.author ?? "未知作者"}</span>
                        <time>{formatRelative(comment.createdAt)}</time>
                        <button
                          className="ml-auto transition-colors hover:text-foreground hover:underline"
                          type="button"
                          onClick={() => void openExternal(comment.url)}
                        >
                          在 GitHub 打开
                        </button>
                      </div>
                      <Markdown className="mt-2" source={comment.body} baseUrl={comment.url} />
                    </article>
                  ))}
                </div>
              )}

              {comments.length > visibleComments ? (
                <div className="mt-3 flex justify-center">
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => setVisibleComments((current) => current + COMMENT_PAGE_SIZE)}
                  >
                    加载更多评论
                  </Button>
                </div>
              ) : null}
            </section>

            <section className="mt-8">
              <SectionTitle>元信息</SectionTitle>
              <div className="mt-2">
                <MetaList thread={thread} />
              </div>
            </section>

            {thread.summary && thread.summary !== thread.body ? (
              <section className="mt-8">
                <SectionTitle>摘要</SectionTitle>
                <p className="mt-3 text-sm leading-7 text-muted-foreground">{thread.summary}</p>
              </section>
            ) : null}
          </article>
        ) : null}
      </div>
    </div>
  );
}

export default ThreadDetailPage;
