import { useCallback, useEffect, useRef, useState } from "react";

import {
  Badge,
  Button,
  ListItem,
  ListItemMeta,
  ListItemTitle,
  Row,
  Tabs,
} from "../../components/ui";
import {
  countInboxPending,
  convertInboxMessageToTask,
  dismissInboxMessage,
  getGithubTokenStatus,
  getThread,
  ingestInboxMessage,
  listInboxMessagesPage,
  listInboxSources,
  markInboxMessageRead,
  markInboxMessageUnread,
  previewMode,
  syncInboxGithub,
  type InboxCounts,
  type InboxMessage,
  type InboxSource,
} from "../../lib/api";
import { notifyInboxChanged, notifyTasksChanged } from "../../lib/events";
import { formatRelative } from "../../lib/format";
import { openExternal } from "../../lib/openExternal";
import InboxMessageDetail from "./InboxMessageDetail";
import { categoryOptions, describeCategory, sourceKindLabels, triageLabels } from "./shared";

type TriagedFilter = "pending" | "triaged" | "all";

const PAGE_SIZE = 50;

const emptyCounts: InboxCounts = { all: 0, pending: 0, unread: 0 };

const emptyHints: Record<TriagedFilter, { title: string; hint: string }> = {
  pending: {
    title: "待判断的消息已经清空",
    hint: "新的通知会在同步后出现在这里，读完可以先放着，判断留到你决定要做什么的时候。",
  },
  triaged: { title: "还没有判断过的消息", hint: "忽略或转为待办之后，消息会留在这里备查。" },
  all: { title: "收件箱是空的", hint: "配置 GitHub Token 后可以同步通知；邮件连接器随后提供。" },
};

/** 开发窗口用来造消息的样例：点一次写一条，循环使用。 */
const devSamples = [
  {
    category: "mention",
    title: "有人在 PR #490 的评论里 @ 了你",
    summary: "这里的游标要不要改成 base64？现在 `|` 偶尔会截断。",
    url: "https://github.com/TencentCloud/Octop/pull/490",
  },
  {
    category: "review_requested",
    title: "请求你评审 PR #488 feat: 收件箱三栏页",
    summary: "筛选与详情都做完了，主要想让人看一下已读与判断的交互。",
    url: "https://github.com/TencentCloud/Octop/pull/488",
  },
  {
    category: "ci_failed",
    title: "v0.4.0 的发布流水线失败",
    summary: "Windows 安装包签名步骤超时，重试一次即可。",
    url: "https://github.com/TencentCloud/Octop/actions/runs/1310",
  },
];

/** 收件箱：来源、消息流、详情三栏；判断动作全部人工触发。 */
function InboxPage({
  onOpenTasks,
  onOpenThread,
  initialMessageId,
}: {
  onOpenTasks: () => void;
  /** 通知已关联到本地线程时跳仓库详情；仓库页由外层切换。 */
  onOpenThread: (repoId: string, threadId: string) => void;
  /** 从待办「查看原消息」进来时定位到指定消息。 */
  initialMessageId?: string | null;
}) {
  const [sources, setSources] = useState<InboxSource[]>([]);
  const [messages, setMessages] = useState<InboxMessage[]>([]);
  const [counts, setCounts] = useState<InboxCounts>(emptyCounts);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  // 带消息 id 进来时先看「全部」，否则已判断的消息不在待判断列表里。
  const [filter, setFilter] = useState<TriagedFilter>(initialMessageId ? "all" : "pending");
  const [unreadOnly, setUnreadOnly] = useState(false);
  const [category, setCategory] = useState("all");
  const [sourceId, setSourceId] = useState<string | null>(null);
  const [cursor, setCursor] = useState<string | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [mutating, setMutating] = useState(false);
  const [ingesting, setIngesting] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [tokenConfigured, setTokenConfigured] = useState(false);
  const requestId = useRef(0);
  const sampleIndex = useRef(0);
  const appliedInitial = useRef(false);

  const refreshCounts = useCallback(async () => {
    try {
      setCounts(await countInboxPending());
    } catch {
      // 角标失败不影响列表，保留上一次的数字。
    }
  }, []);

  const load = useCallback(async () => {
    const currentRequest = ++requestId.current;
    setLoading(true);
    setError(null);
    setNotice(null);
    try {
      const [page, nextCounts] = await Promise.all([
        listInboxMessagesPage({
          triaged: filter === "all" ? null : filter === "pending" ? false : true,
          unreadOnly,
          category: category === "all" ? null : category,
          sourceId,
          limit: PAGE_SIZE,
        }),
        countInboxPending(),
      ]);
      if (currentRequest !== requestId.current) return;
      setMessages(page.messages);
      setCursor(page.nextCursor);
      setHasMore(page.hasMore);
      setCounts(nextCounts);
    } catch (loadError) {
      if (currentRequest !== requestId.current) return;
      setMessages([]);
      setCursor(null);
      setHasMore(false);
      setError(loadError instanceof Error ? loadError.message : "加载收件箱失败，请重试。");
    } finally {
      if (currentRequest === requestId.current) setLoading(false);
    }
  }, [category, filter, sourceId, unreadOnly]);

  useEffect(() => {
    void load();
  }, [load]);

  // 消息源是只读展示，进页面取一次；开发用 ingest 新建来源后要再取一次。
  const loadSources = useCallback(async () => {
    try {
      setSources(await listInboxSources());
    } catch {
      setSources([]);
    }
  }, []);

  useEffect(() => {
    void loadSources();
  }, [loadSources]);

  // 没有 Token 时不暴露同步按钮，避免点下去必然失败。
  useEffect(() => {
    void getGithubTokenStatus()
      .then((status) => setTokenConfigured(status.configured))
      .catch(() => setTokenConfigured(false));
  }, []);

  const loadMore = useCallback(async () => {
    if (!hasMore || loadingMore || loading || !cursor) return;
    const currentRequest = requestId.current;
    setLoadingMore(true);
    try {
      const page = await listInboxMessagesPage({
        triaged: filter === "all" ? null : filter === "pending" ? false : true,
        unreadOnly,
        category: category === "all" ? null : category,
        sourceId,
        cursor,
        limit: PAGE_SIZE,
      });
      if (currentRequest !== requestId.current) return;
      setMessages((current) => {
        const seen = new Set(current.map((message) => message.id));
        return [...current, ...page.messages.filter((message) => !seen.has(message.id))];
      });
      setCursor(page.nextCursor);
      setHasMore(page.hasMore);
    } catch (loadError) {
      if (currentRequest !== requestId.current) return;
      setError(loadError instanceof Error ? loadError.message : "加载更多失败，请重试。");
    } finally {
      setLoadingMore(false);
    }
  }, [category, cursor, filter, hasMore, loading, loadingMore, sourceId, unreadOnly]);

  const matchesFilters = useCallback(
    (message: InboxMessage) => {
      if (filter === "pending" && message.triagedAt) return false;
      if (filter === "triaged" && !message.triagedAt) return false;
      if (unreadOnly && message.readAt) return false;
      if (category !== "all" && message.category !== category) return false;
      if (sourceId && message.sourceId !== sourceId) return false;
      return true;
    },
    [category, filter, sourceId, unreadOnly],
  );

  /** 单条变化后按当前筛选就地更新或移除，避免整页重载。 */
  const applyMessage = useCallback(
    (updated: InboxMessage) => {
      setMessages((current) => {
        if (!matchesFilters(updated)) return current.filter((message) => message.id !== updated.id);
        return current.map((message) => (message.id === updated.id ? updated : message));
      });
    },
    [matchesFilters],
  );

  useEffect(() => {
    if (messages.length === 0) {
      if (selectedId !== null) setSelectedId(null);
      return;
    }
    // 从待办回跳时只应用一次，之后用户自己选哪条就停在哪条。
    if (
      !appliedInitial.current &&
      initialMessageId &&
      messages.some((message) => message.id === initialMessageId)
    ) {
      appliedInitial.current = true;
      setSelectedId(initialMessageId);
      return;
    }
    if (!messages.some((message) => message.id === selectedId)) {
      setSelectedId(messages[0].id);
    }
  }, [initialMessageId, messages, selectedId]);

  const selectedMessage = messages.find((message) => message.id === selectedId) ?? null;
  const selectedSource = selectedMessage
    ? (sources.find((source) => source.id === selectedMessage.sourceId) ?? null)
    : null;

  /**
   * 点开未读消息即视为读过；失败不打断阅读。
   * 「只看未读」下不自动标已读：那样读一条就从列表里消失一条，反而打断扫读。
   */
  async function selectMessage(message: InboxMessage) {
    setSelectedId(message.id);
    if (message.readAt || unreadOnly) return;
    try {
      const updated = await markInboxMessageRead(message.id);
      applyMessage(updated);
      await refreshCounts();
      notifyInboxChanged();
    } catch {
      // 已读只是辅助状态，失败保留未读即可。
    }
  }

  async function toggleRead() {
    if (!selectedMessage || mutating) return;
    setMutating(true);
    setError(null);
    try {
      const updated = selectedMessage.readAt
        ? await markInboxMessageUnread(selectedMessage.id)
        : await markInboxMessageRead(selectedMessage.id);
      applyMessage(updated);
      await refreshCounts();
      notifyInboxChanged();
    } catch (updateError) {
      setError(updateError instanceof Error ? updateError.message : "更新已读状态失败，请重试。");
    } finally {
      setMutating(false);
    }
  }

  async function dismiss() {
    if (!selectedMessage || mutating) return;
    setMutating(true);
    setError(null);
    try {
      const updated = await dismissInboxMessage(selectedMessage.id);
      applyMessage(updated);
      await refreshCounts();
      notifyInboxChanged();
    } catch (updateError) {
      setError(updateError instanceof Error ? updateError.message : "忽略消息失败，请重试。");
    } finally {
      setMutating(false);
    }
  }

  async function convert() {
    if (!selectedMessage || mutating) return;
    setMutating(true);
    setError(null);
    try {
      const task = await convertInboxMessageToTask(selectedMessage.id);
      // 转换只返回待办，消息的判断字段要重载列表才拿得到；load 会清掉旧提示，提示放在它之后。
      await load();
      setNotice(`已转为待办：${task.title}`);
      notifyInboxChanged();
      notifyTasksChanged();
    } catch (updateError) {
      setError(updateError instanceof Error ? updateError.message : "转为待办失败，请重试。");
    } finally {
      setMutating(false);
    }
  }

  async function openSelectedLink() {
    if (!selectedMessage?.url) return;
    try {
      await openExternal(selectedMessage.url);
    } catch {
      setError("打开链接失败，请复制 URL 后重试。");
    }
  }

  /** 命中本地线程时跳仓库详情，未同步时退回 GitHub 原链接。 */
  async function openRelated() {
    const message = selectedMessage;
    if (!message || message.relatedKind !== "github_thread" || !message.relatedId) return;
    try {
      const thread = await getThread(message.relatedId);
      onOpenThread(thread.repoId, thread.id);
    } catch {
      setError("本地线程不存在或尚未同步，已改为在 GitHub 打开。");
      if (message.url) {
        try {
          await openExternal(message.url);
        } catch {
          // 外部链接也失败时保留提示，用户可以手工复制 URL。
        }
      }
    }
  }

  async function syncGitHub() {
    if (syncing || mutating) return;
    setSyncing(true);
    setError(null);
    setNotice(null);
    try {
      const summary = await syncInboxGithub();
      // load 会清掉旧提示，所以先刷新再写结果。
      await Promise.all([load(), loadSources()]);
      setNotice(
        summary.fetchedCount === 0
          ? "没有新的 GitHub 通知。"
          : `同步完成：拉到 ${summary.fetchedCount} 条通知，待判断 ${summary.pendingCount} 条。`,
      );
      notifyInboxChanged();
    } catch (syncError) {
      setError(syncError instanceof Error ? syncError.message : "同步 GitHub 通知失败，请重试。");
    } finally {
      setSyncing(false);
    }
  }

  async function ingestSample() {
    const sample = devSamples[sampleIndex.current % devSamples.length];
    sampleIndex.current += 1;
    setIngesting(true);
    setError(null);
    try {
      await ingestInboxMessage({
        externalId: `dev-${Date.now()}`,
        category: sample.category,
        title: sample.title,
        summary: sample.summary,
        url: sample.url,
        actor: "开发窗口",
        occurredAt: new Date().toISOString(),
      });
      await load();
      await loadSources();
      notifyInboxChanged();
    } catch (ingestError) {
      setError(ingestError instanceof Error ? ingestError.message : "造消息失败，请重试。");
    } finally {
      setIngesting(false);
    }
  }

  const tabs = [
    { id: "pending", label: "待判断", count: counts.pending },
    { id: "triaged", label: "已判断", count: counts.all - counts.pending },
    { id: "all", label: "全部", count: counts.all },
  ];

  const empty = unreadOnly
    ? {
        title: "没有未读消息",
        hint: "取消「只看未读」可以看到当前筛选下的全部消息。",
      }
    : emptyHints[filter];

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <header className="px-5 pt-4 sm:px-7">
        <div className="flex flex-wrap items-baseline justify-between gap-3">
          <div className="flex items-baseline gap-3">
            <h2 className="text-lg font-bold tracking-tight">收件箱</h2>
            <span className="font-mono text-xs text-muted-foreground">
              待判断 {counts.pending} · 未读 {counts.unread}
            </span>
          </div>
          <div className="flex items-center gap-2">
            {import.meta.env.DEV && !previewMode ? (
              <Button
                variant="ghost"
                size="sm"
                disabled={ingesting}
                onClick={() => void ingestSample()}
              >
                {ingesting ? "写入中…" : "造一条示例消息"}
              </Button>
            ) : null}
            <Button variant="ghost" size="sm" onClick={() => void load()}>
              刷新
            </Button>
            {tokenConfigured ? (
              <Button
                variant="outline"
                size="sm"
                disabled={syncing}
                onClick={() => void syncGitHub()}
              >
                {syncing ? "同步中…" : "同步 GitHub"}
              </Button>
            ) : null}
          </div>
        </div>

        <div className="mt-3">
          <Tabs
            items={tabs}
            value={filter}
            onChange={(id) => setFilter(id as TriagedFilter)}
            trailing={
              <>
                <button
                  type="button"
                  aria-pressed={unreadOnly}
                  className={
                    unreadOnly
                      ? "h-7 rounded-md border border-input bg-input/20 px-2 font-mono text-xs text-foreground"
                      : "h-7 rounded-md border border-transparent px-2 font-mono text-xs text-muted-foreground hover:text-foreground"
                  }
                  onClick={() => setUnreadOnly((current) => !current)}
                >
                  只看未读
                </button>
                <select
                  className="h-7 rounded-md bg-transparent px-1.5 font-mono text-xs text-muted-foreground outline-none hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
                  value={category}
                  onChange={(event) => setCategory(event.target.value)}
                >
                  <option value="all">全部类型</option>
                  {categoryOptions.map((option) => (
                    <option key={option.value} value={option.value}>
                      {option.label}
                    </option>
                  ))}
                </select>
                <select
                  className="h-7 rounded-md bg-transparent px-1.5 font-mono text-xs text-muted-foreground outline-none hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50 lg:hidden"
                  value={sourceId ?? "all"}
                  onChange={(event) =>
                    setSourceId(event.target.value === "all" ? null : event.target.value)
                  }
                >
                  <option value="all">全部来源</option>
                  {sources.map((source) => (
                    <option key={source.id} value={source.id}>
                      {source.name}
                    </option>
                  ))}
                </select>
              </>
            }
          />
        </div>
      </header>

      {error ? (
        <div className="flex flex-wrap items-center justify-between gap-3 callout callout-error border-b border-border px-5 py-2.5 text-sm text-destructive sm:px-7">
          <p>{error}</p>
          <button
            className="text-xs font-medium underline"
            type="button"
            onClick={() => void load()}
          >
            重新加载
          </button>
        </div>
      ) : null}
      {notice ? (
        <div className="flex flex-wrap items-center justify-between gap-3 callout callout-success border-b border-border px-5 py-2.5 text-sm text-success sm:px-7">
          <p>{notice}</p>
          <div className="flex items-center gap-3">
            <button className="text-xs font-medium underline" type="button" onClick={onOpenTasks}>
              去待办页
            </button>
            <button
              className="text-xs font-medium underline"
              type="button"
              onClick={() => setNotice(null)}
            >
              知道了
            </button>
          </div>
        </div>
      ) : null}

      <div className="grid min-h-0 flex-1 lg:grid-cols-[13rem_minmax(17rem,0.95fr)_minmax(21rem,1.05fr)]">
        <aside
          className="hidden min-h-0 overflow-y-auto border-r border-border px-3 py-3 lg:block"
          aria-label="消息源"
        >
          <p className="px-3 py-1 font-mono text-xs text-muted-foreground">消息源</p>
          <Row active={sourceId === null} onClick={() => setSourceId(null)}>
            全部来源
          </Row>
          {sources.map((source) => (
            <Row
              key={source.id}
              active={sourceId === source.id}
              onClick={() => setSourceId(source.id)}
              trailing={
                <span className="font-mono text-xs text-muted-foreground">
                  {sourceKindLabels[source.kind]}
                </span>
              }
            >
              {source.name}
            </Row>
          ))}
          <p className="mt-4 px-3 text-xs leading-5 text-muted-foreground">
            消息源只读展示，同步与新增来源在接入连接器后提供。
          </p>
        </aside>

        <section
          className="min-h-72 overflow-y-auto border-b border-border lg:border-r lg:border-b-0"
          aria-label="消息列表"
          aria-busy={loading}
        >
          {loading ? (
            <div className="divide-y divide-border" role="status" aria-label="正在加载">
              {Array.from({ length: 6 }).map((_, index) => (
                <div key={index} className="space-y-2 px-4 py-3" aria-hidden>
                  <div className="h-3.5 w-4/5 animate-pulse rounded bg-muted" />
                  <div className="h-3 w-2/3 animate-pulse rounded bg-muted/70" />
                </div>
              ))}
            </div>
          ) : null}

          {!loading && !error && messages.length === 0 ? (
            <div className="px-4 py-10 text-center">
              <p className="text-sm text-foreground">{empty.title}</p>
              <p className="mt-1 text-sm leading-6 text-muted-foreground">{empty.hint}</p>
            </div>
          ) : null}

          <div className="divide-y divide-border">
            {messages.map((message) => (
              <ListItem
                key={message.id}
                active={message.id === selectedId}
                flush
                className="px-4 py-3"
                aria-current={message.id === selectedId ? "true" : undefined}
                onClick={() => void selectMessage(message)}
              >
                <span className="flex items-start gap-2">
                  {message.readAt ? null : (
                    <span aria-hidden className="mt-1.5 size-1.5 shrink-0 rounded-full bg-link" />
                  )}
                  <ListItemTitle clamp={2}>{message.title}</ListItemTitle>
                </span>
                {message.summary ? (
                  <p className="line-clamp-1 text-sm text-muted-foreground">{message.summary}</p>
                ) : null}
                <ListItemMeta>
                  <span>{describeCategory(message.category)}</span>
                  {message.actor ? <span>{message.actor}</span> : null}
                  {message.triageOutcome ? (
                    <Badge tone="neutral">{triageLabels[message.triageOutcome]}</Badge>
                  ) : null}
                  <time className="ml-auto" dateTime={message.occurredAt}>
                    {formatRelative(message.occurredAt)}
                  </time>
                </ListItemMeta>
              </ListItem>
            ))}
          </div>

          {!loading && !error ? (
            <div className="flex justify-center px-4 py-4">
              {loadingMore ? (
                <span className="font-mono text-xs text-muted-foreground">正在加载更多…</span>
              ) : hasMore ? (
                <Button variant="outline" size="sm" onClick={() => void loadMore()}>
                  加载更多
                </Button>
              ) : messages.length > 0 ? (
                <span className="font-mono text-xs text-muted-foreground">已显示全部</span>
              ) : null}
            </div>
          ) : null}
        </section>

        <InboxMessageDetail
          message={selectedMessage}
          source={selectedSource}
          mutating={mutating}
          onToggleRead={() => void toggleRead()}
          onDismiss={() => void dismiss()}
          onConvert={() => void convert()}
          onOpenLink={() => void openSelectedLink()}
          onOpenLocal={
            selectedMessage?.relatedKind === "github_thread" && selectedMessage.relatedId
              ? () => void openRelated()
              : null
          }
        />
      </div>
    </div>
  );
}

export default InboxPage;
