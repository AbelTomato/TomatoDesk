import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

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
  applyInboxAction,
  countInboxMessages,
  convertInboxMessageToTask,
  getInboxReadBackStatus,
  getThread,
  ingestInboxMessage,
  listInboxMessagesPage,
  listInboxSources,
  markInboxMessageRead,
  markInboxMessageUnread,
  previewMode,
  undoInboxAction,
  type ApplyInboxActionInput,
  type InboxAction,
  type InboxActionResult,
  type InboxCounts,
  type InboxMessage,
  type InboxReadBackStatus,
  type InboxScopeFilter,
  type InboxSource,
  type InboxSyncSummary,
  type InboxTab,
} from "../../lib/api";
import { notifyInboxChanged, notifyTasksChanged } from "../../lib/events";
import { formatRelative } from "../../lib/format";
import { openExternal } from "../../lib/openExternal";
import BulkActionBar, { ConfirmDialog } from "./BulkActionBar";
import InboxMessageDetail from "./InboxMessageDetail";
import {
  emptySelection,
  isEmpty,
  isSelected,
  selectScope,
  selectVisible,
  summary as describeSelection,
  toggle,
  type InboxSelection,
} from "./inboxSelection";
import {
  actionDoneLabels,
  actionLabels,
  categoryOptions,
  describeCategory,
  emptyHints,
  inboxTabOrder,
  latestSyncedAt,
  sourceKindLabels,
  tabLabels,
  triageLabels,
} from "./shared";

const PAGE_SIZE = 50;
/** 超过这个条数才为「转为待办」弹确认框：小批量误点还能自己取消，大批量很难。 */
const TASK_CONFIRM_THRESHOLD = 20;
/** 撤销窗口：够按一次，也不至于让人以为它一直在。 */
const UNDO_WINDOW_MS = 8000;

const emptyCounts: InboxCounts = { all: 0, pending: 0, task: 0, dismissed: 0, unread: 0 };

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
    summary: "筛选与详情都做完了，主要想让人看一下已读与处理的交互。",
    url: "https://github.com/TencentCloud/Octop/pull/488",
  },
  {
    category: "ci_failed",
    title: "v0.4.0 的发布流水线失败",
    summary: "Windows 安装包签名步骤超时，重试一次即可。",
    url: "https://github.com/TencentCloud/Octop/actions/runs/1310",
  },
];

/** 最近一次动作，撤销窗口内一直留着；离开页面或超时即失效。 */
interface UndoState {
  action: InboxAction;
  ids: string[];
  taskIds: string[];
}

/** 需要先说清后果才能继续的两件事：批量转待办、条数变了。 */
type ConfirmState =
  { kind: "task"; ids: string[] } | { kind: "stale"; action: InboxAction; expected: number };

/** 收件箱：来源、消息流、详情三栏；处理动作全部人工触发。 */
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
  // 带消息 id 进来时先看「全部」，否则已处理的消息不在待处理列表里。
  const [filter, setFilter] = useState<InboxTab>(initialMessageId ? "all" : "pending");
  const [unreadOnly, setUnreadOnly] = useState(false);
  const [category, setCategory] = useState("all");
  const [sourceId, setSourceId] = useState<string | null>(null);
  const [cursor, setCursor] = useState<string | null>(null);
  const [hasMore, setHasMore] = useState(false);
  /** 当前筛选下的总条数，只用于「选择全部 N 条」那一步。 */
  const [scopeTotal, setScopeTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [mutating, setMutating] = useState(false);
  const [ingesting, setIngesting] = useState(false);
  const [selecting, setSelecting] = useState(false);
  const [selection, setSelection] = useState<InboxSelection>(emptySelection);
  const [bulkBusy, setBulkBusy] = useState(false);
  const [confirm, setConfirm] = useState<ConfirmState | null>(null);
  const [undo, setUndo] = useState<UndoState | null>(null);
  const [readBack, setReadBack] = useState<InboxReadBackStatus | null>(null);
  const requestId = useRef(0);
  const sampleIndex = useRef(0);
  const appliedInitial = useRef(false);

  /**
   * 计数只随来源与分类变化，不随 Tab 与未读变化：
   * 这样同一来源下四个 Tab 的数字加起来等于「全部」。
   */
  const countFilter = useMemo(
    () => ({ sourceId, category: category === "all" ? null : category }),
    [category, sourceId],
  );

  /** 列表与「选择全部」共用同一套筛选，避免两处慢慢漂移。 */
  const currentScope = useMemo<InboxScopeFilter>(
    () => ({
      tab: filter,
      unreadOnly,
      category: category === "all" ? null : category,
      sourceId,
    }),
    [category, filter, sourceId, unreadOnly],
  );

  const refreshCounts = useCallback(async () => {
    try {
      setCounts(await countInboxMessages(countFilter));
    } catch {
      // 角标失败不影响列表，保留上一次的数字。
    }
  }, [countFilter]);

  const load = useCallback(async () => {
    const currentRequest = ++requestId.current;
    setLoading(true);
    setError(null);
    setNotice(null);
    try {
      const [page, nextCounts] = await Promise.all([
        listInboxMessagesPage({
          tab: filter,
          unreadOnly,
          category: category === "all" ? null : category,
          sourceId,
          limit: PAGE_SIZE,
        }),
        countInboxMessages(countFilter),
      ]);
      if (currentRequest !== requestId.current) return;
      setMessages(page.messages);
      setCursor(page.nextCursor);
      setHasMore(page.hasMore);
      setScopeTotal(page.total);
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
  }, [category, countFilter, filter, sourceId, unreadOnly]);

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

  // 已读回写失败不阻塞本地已读，但要在消息源栏提示一句，避免「已读」悄悄没同步出去。
  const loadReadBack = useCallback(async () => {
    try {
      setReadBack(await getInboxReadBackStatus());
    } catch {
      setReadBack(null);
    }
  }, []);

  useEffect(() => {
    void loadReadBack();
  }, [loadReadBack]);

  /**
   * 通知由后台每五分钟轮询一次，同步完成后发事件过来。
   * 这里静默重载：用户正在读的那条如果还在列表里，选中项不会跳走。
   */
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    const stops: Array<() => void> = [];
    void getCurrentWindow()
      .listen<InboxSyncSummary | null>("inbox-sync-updated", () => {
        if (disposed) return;
        void load();
        void loadSources();
      })
      .then((stop) => {
        if (disposed) stop();
        else stops.push(stop);
      });
    void getCurrentWindow()
      .listen("inbox-read-back-updated", () => {
        if (!disposed) void loadReadBack();
      })
      .then((stop) => {
        if (disposed) stop();
        else stops.push(stop);
      });
    return () => {
      disposed = true;
      stops.forEach((stop) => stop());
    };
  }, [load, loadReadBack, loadSources]);

  // 换筛选或切 Tab 时退出选择态：否则「选择全部」会带着上一屏的条数去执行。
  useEffect(() => {
    setSelecting(false);
    setSelection(emptySelection);
    setConfirm(null);
  }, [category, filter, sourceId, unreadOnly]);

  // 撤销窗口：一次动作只留最近一次，8 秒后自己消失。
  useEffect(() => {
    if (!undo) return;
    const timer = window.setTimeout(() => setUndo(null), UNDO_WINDOW_MS);
    return () => window.clearTimeout(timer);
  }, [undo]);

  const loadMore = useCallback(async () => {
    if (!hasMore || loadingMore || loading || !cursor) return;
    const currentRequest = requestId.current;
    setLoadingMore(true);
    try {
      const page = await listInboxMessagesPage({
        tab: filter,
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
      setScopeTotal(page.total);
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
      if (filter === "task" && message.triageOutcome !== "task") return false;
      if (filter === "dismissed" && message.triageOutcome !== "dismissed") return false;
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
  // 同步时间取所有启用来源里最新的一次，GitHub 与多个邮箱账户共用同一个位置。
  const lastSyncedAt = latestSyncedAt(sources);
  const failingSources = sources.filter((source) => source.enabled && source.lastError);

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

  /** 详情页的「忽略」就是只勾了一条的批量动作，不另开一条路径。 */
  async function dismiss() {
    if (!selectedMessage || mutating || bulkBusy) return;
    await submitAction("dismiss", { ids: [selectedMessage.id] });
  }

  async function convert() {
    if (!selectedMessage || mutating) return;
    setMutating(true);
    setError(null);
    try {
      const task = await convertInboxMessageToTask(selectedMessage.id);
      // 转换只返回待办，消息的处理字段要重载列表才拿得到；load 会清掉旧提示，提示放在它之后。
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

  /** 乐观更新：只改真正变了的行，再按当前筛选决定留还是走。 */
  function applyLocally(action: InboxAction, result: InboxActionResult) {
    const touched = new Set(result.appliedIds);
    const taskByMessage = new Map(
      result.appliedIds.map((id, index) => [id, result.taskIds[index] ?? null]),
    );
    const now = new Date().toISOString();
    setMessages((current) =>
      current
        .map((message) => {
          if (!touched.has(message.id)) return message;
          switch (action) {
            case "read":
              return { ...message, readAt: message.readAt ?? now };
            case "unread":
              return { ...message, readAt: null };
            case "dismiss":
              return {
                ...message,
                triagedAt: now,
                triageOutcome: "dismissed" as const,
                triageRefId: null,
              };
            case "task":
              return {
                ...message,
                triagedAt: now,
                triageOutcome: "task" as const,
                triageRefId: taskByMessage.get(message.id) ?? null,
              };
          }
        })
        .filter(matchesFilters),
    );
  }

  function exitSelecting() {
    setSelecting(false);
    setSelection(emptySelection);
  }

  /**
   * 所有动作的唯一入口：单条、勾选、跨页全选都走这里。
   * 条数变了只弹确认框，不改任何一行；确认后再带上新的条数重试一次。
   */
  async function submitAction(action: InboxAction, input: Omit<ApplyInboxActionInput, "action">) {
    setBulkBusy(true);
    setError(null);
    try {
      const result = await applyInboxAction({ action, ...input });
      if (result.staleCount !== null) {
        setConfirm({ kind: "stale", action, expected: result.staleCount });
        return;
      }
      applyLocally(action, result);
      setScopeTotal((current) => Math.max(0, current - result.appliedIds.length));
      if (result.appliedIds.length > 0) {
        setUndo({ action, ids: result.appliedIds, taskIds: result.taskIds });
      }
      setConfirm(null);
      exitSelecting();
      await refreshCounts();
      notifyInboxChanged();
      if (action === "task") notifyTasksChanged();
    } catch (actionError) {
      setError(actionError instanceof Error ? actionError.message : "处理失败，请重试。");
      // 本地列表可能已经乐观改过一半，直接按后端重载。
      await load();
    } finally {
      setBulkBusy(false);
    }
  }

  function handleBulkAction(action: InboxAction) {
    if (isEmpty(selection)) return;
    if (selection.mode === "scope") {
      void submitAction(action, {
        scope: currentScope,
        expectedCount: selection.total,
      });
      return;
    }
    if (action === "task" && selection.ids.length > TASK_CONFIRM_THRESHOLD) {
      setConfirm({ kind: "task", ids: selection.ids });
      return;
    }
    void submitAction(action, { ids: selection.ids });
  }

  async function runUndo(state: UndoState) {
    setBulkBusy(true);
    setError(null);
    try {
      await undoInboxAction({ action: state.action, ids: state.ids, taskIds: state.taskIds });
      setUndo(null);
      // 撤销要回到原状，手工回填容易错位，直接重载一次更稳。
      await load();
      notifyInboxChanged();
      if (state.action === "task") notifyTasksChanged();
    } catch (undoError) {
      setError(undoError instanceof Error ? undoError.message : "撤销失败，请重试。");
    } finally {
      setBulkBusy(false);
    }
  }

  /** 详情页的撤销：只有一条，动作由处理结果反推。 */
  async function undoSelected() {
    if (!selectedMessage?.triageOutcome || mutating) return;
    const action: InboxAction = selectedMessage.triageOutcome === "task" ? "task" : "dismiss";
    const taskIds =
      action === "task" && selectedMessage.triageRefId ? [selectedMessage.triageRefId] : [];
    setMutating(true);
    setError(null);
    try {
      await undoInboxAction({ action, ids: [selectedMessage.id], taskIds });
      await load();
      notifyInboxChanged();
      if (action === "task") notifyTasksChanged();
    } catch (undoError) {
      setError(undoError instanceof Error ? undoError.message : "撤销失败，请重试。");
    } finally {
      setMutating(false);
    }
  }

  // 计数直接从各自条件算出，不再用 all - pending 凑数。
  const tabCounts: Record<InboxTab, number> = {
    pending: counts.pending,
    task: counts.task,
    dismissed: counts.dismissed,
    all: counts.all,
  };
  const tabs = inboxTabOrder.map((tab) => ({
    id: tab,
    label: tabLabels[tab],
    count: tabCounts[tab],
  }));

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
            {lastSyncedAt ? (
              <span className="font-mono text-xs text-muted-foreground">
                {formatRelative(lastSyncedAt)}同步
              </span>
            ) : null}
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
            <Button
              variant="ghost"
              size="sm"
              disabled={messages.length === 0}
              aria-pressed={selecting}
              onClick={() => (selecting ? exitSelecting() : setSelecting(true))}
            >
              {selecting ? "退出选择" : "选择"}
            </Button>
            <Button variant="ghost" size="sm" onClick={() => void load()}>
              刷新
            </Button>
          </div>
        </div>

        <div className="mt-3">
          <Tabs
            items={tabs}
            value={filter}
            onChange={(id) => setFilter(id as InboxTab)}
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
      {undo ? (
        <div className="flex flex-wrap items-center justify-between gap-3 callout callout-success border-b border-border px-5 py-2.5 text-sm text-success sm:px-7">
          <p>
            {actionDoneLabels[undo.action]} {undo.ids.length} 条
          </p>
          <div className="flex items-center gap-3">
            <button
              className="text-xs font-medium underline"
              type="button"
              disabled={bulkBusy}
              onClick={() => void runUndo(undo)}
            >
              撤销
            </button>
            <button
              className="text-xs font-medium underline"
              type="button"
              onClick={() => setUndo(null)}
            >
              知道了
            </button>
          </div>
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
              // 邮箱来源用地址当名字，悬停给出完整地址。
              title={source.name}
              trailing={
                // 地址本身已经说明是邮箱，省下这段宽度让域名完整显示。
                source.kind === "email" ? null : (
                  <span className="font-mono text-xs text-muted-foreground">
                    {sourceKindLabels[source.kind]}
                  </span>
                )
              }
            >
              {/* 邮箱来源的 name 就是地址，用等宽小号才放得下完整域名。 */}
              {source.kind === "email" ? (
                <span className="font-mono text-xs">{source.name}</span>
              ) : (
                source.name
              )}
            </Row>
          ))}
          {failingSources.map((source) => (
            <p key={source.id} className="mt-2 px-3 text-xs leading-5 text-destructive">
              {source.name} 同步失败：{source.lastError}
            </p>
          ))}
          {readBack && readBack.blocked > 0 ? (
            <p className="mt-2 px-3 text-xs leading-5 text-warning">
              {readBack.blocked} 条已读没能同步给 GitHub，到设置页确认权限后可重试。
            </p>
          ) : null}
        </aside>

        <section
          className={
            selecting
              ? "min-h-72 overflow-y-auto border-b border-border pb-20 lg:border-r lg:border-b-0"
              : "min-h-72 overflow-y-auto border-b border-border lg:border-r lg:border-b-0"
          }
          aria-label="消息列表"
          aria-busy={loading}
        >
          {selecting ? (
            <div className="flex items-center justify-between gap-3 border-b border-border px-4 py-2">
              <span className="font-mono text-xs text-muted-foreground">
                {describeSelection(selection)}
              </span>
              <div className="flex items-center gap-3">
                <button
                  className="text-xs font-medium text-muted-foreground underline hover:text-foreground"
                  type="button"
                  onClick={() => setSelection(selectVisible(messages.map((item) => item.id)))}
                >
                  全选本页
                </button>
                <button
                  className="text-xs font-medium text-muted-foreground underline hover:text-foreground"
                  type="button"
                  onClick={() => setSelection(emptySelection)}
                >
                  清空
                </button>
              </div>
            </div>
          ) : null}

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
                onClick={() =>
                  selecting
                    ? setSelection(
                        toggle(
                          selection,
                          message.id,
                          messages.map((item) => item.id),
                        ),
                      )
                    : void selectMessage(message)
                }
              >
                <span className="flex items-start gap-2">
                  {selecting ? (
                    <SelectionMark selected={isSelected(selection, message.id)} />
                  ) : message.readAt ? null : (
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
                  {message.triageOutcome && filter === "all" ? (
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
          mutating={mutating || bulkBusy}
          onToggleRead={() => void toggleRead()}
          onDismiss={() => void dismiss()}
          onConvert={() => void convert()}
          onUndo={() => void undoSelected()}
          onOpenLink={() => void openSelectedLink()}
          onOpenLocal={
            selectedMessage?.relatedKind === "github_thread" && selectedMessage.relatedId
              ? () => void openRelated()
              : null
          }
        />
      </div>

      {selecting ? (
        <BulkActionBar
          selection={selection}
          hasMore={hasMore}
          scopeTotal={scopeTotal}
          busy={bulkBusy}
          onSelectScope={() => setSelection(selectScope(scopeTotal))}
          onAction={handleBulkAction}
          onClear={exitSelecting}
        />
      ) : null}

      {confirm ? (
        <ConfirmDialog
          title={confirm.kind === "stale" ? "条数变了" : `转为待办 ${confirm.ids.length} 条`}
          description={
            confirm.kind === "stale"
              ? `现在是 ${confirm.expected} 条，仍要${actionLabels[confirm.action]}吗？`
              : "会按消息标题新建待办。撤销只解除关联，待办会留下。"
          }
          details={
            confirm.kind === "task"
              ? confirm.ids
                  .slice(0, 5)
                  .map((id) => messages.find((message) => message.id === id)?.title ?? id)
              : undefined
          }
          confirmLabel={confirm.kind === "stale" ? "仍要处理" : "转为待办"}
          busy={bulkBusy}
          onCancel={() => setConfirm(null)}
          onConfirm={() => {
            if (confirm.kind === "stale") {
              void submitAction(confirm.action, {
                scope: currentScope,
                expectedCount: confirm.expected,
              });
              return;
            }
            void submitAction("task", { ids: confirm.ids });
          }}
        >
          {confirm.kind === "task" && confirm.ids.length > 5 ? (
            <p className="mt-2 font-mono text-xs text-muted-foreground">
              还有 {confirm.ids.length - 5} 条未列出
            </p>
          ) : null}
        </ConfirmDialog>
      ) : null}
    </div>
  );
}

/** 选择态的勾选框：填充表示已选，不用字符，避免字号进到 10px 以下。 */
function SelectionMark({ selected }: { selected: boolean }) {
  return (
    <span
      aria-hidden
      className={
        selected
          ? "mt-1 size-3.5 shrink-0 rounded-sm border border-link bg-link"
          : "mt-1 size-3.5 shrink-0 rounded-sm border border-input"
      }
    />
  );
}

export default InboxPage;
