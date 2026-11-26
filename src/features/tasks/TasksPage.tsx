import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";

import {
  Badge,
  Button,
  Input,
  ListItem,
  ListItemMeta,
  ListItemTitle,
  Tabs,
} from "../../components/ui";
import {
  cancelTask,
  completeTask,
  countTasks,
  createTask,
  listTasksPage,
  reopenTask,
  updateTask,
  type Task,
  type TaskCounts,
} from "../../lib/api";
import { notifyTasksChanged } from "../../lib/events";
import { formatRelative } from "../../lib/format";
import TaskDetail from "./TaskDetail";
import { describeOrigin, taskNotePreview, taskStatusLabels, taskStatusTones } from "./shared";

type StatusFilter = "open" | "done" | "cancelled" | "all";

const PAGE_SIZE = 50;

const emptyCounts: TaskCounts = { open: 0, done: 0, cancelled: 0, all: 0 };

const emptyHints: Record<StatusFilter, { title: string; hint: string }> = {
  open: { title: "没有进行中的待办", hint: "在上面输入标题，回车即可添加。" },
  done: { title: "还没有完成的待办", hint: "完成一条待办后会归档到这里，可随时重开。" },
  cancelled: { title: "没有已取消的待办", hint: "取消只是标记，条目仍保留在全部里。" },
  all: { title: "还没有待办", hint: "先写下第一件事，之后可以随时完成或取消。" },
};

/** 待办：清单 + 详情，字段只保留标题、备注、状态与来源。 */
function TasksPage({ onOpenMessage }: { onOpenMessage: (id: string) => void }) {
  const [tasks, setTasks] = useState<Task[]>([]);
  const [counts, setCounts] = useState<TaskCounts>(emptyCounts);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [filter, setFilter] = useState<StatusFilter>("open");
  const [cursor, setCursor] = useState<string | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [newTitle, setNewTitle] = useState("");
  const [creating, setCreating] = useState(false);
  const [mutating, setMutating] = useState(false);
  const requestId = useRef(0);

  const load = useCallback(async () => {
    const currentRequest = ++requestId.current;
    setLoading(true);
    setError(null);
    try {
      const [page, nextCounts] = await Promise.all([
        listTasksPage({ status: filter === "all" ? null : filter, limit: PAGE_SIZE }),
        countTasks(),
      ]);
      if (currentRequest !== requestId.current) return;
      setTasks(page.tasks);
      setCursor(page.nextCursor);
      setHasMore(page.hasMore);
      setCounts(nextCounts);
    } catch (loadError) {
      if (currentRequest !== requestId.current) return;
      setTasks([]);
      setCursor(null);
      setHasMore(false);
      setError(loadError instanceof Error ? loadError.message : "加载待办失败，请重试。");
    } finally {
      if (currentRequest === requestId.current) setLoading(false);
    }
  }, [filter]);

  useEffect(() => {
    void load();
  }, [load]);

  const loadMore = useCallback(async () => {
    if (!hasMore || loadingMore || loading || !cursor) return;
    const currentRequest = requestId.current;
    setLoadingMore(true);
    try {
      const page = await listTasksPage({
        status: filter === "all" ? null : filter,
        cursor,
        limit: PAGE_SIZE,
      });
      if (currentRequest !== requestId.current) return;
      setTasks((current) => {
        const seen = new Set(current.map((task) => task.id));
        return [...current, ...page.tasks.filter((task) => !seen.has(task.id))];
      });
      setCursor(page.nextCursor);
      setHasMore(page.hasMore);
    } catch (loadError) {
      if (currentRequest !== requestId.current) return;
      setError(loadError instanceof Error ? loadError.message : "加载更多失败，请重试。");
    } finally {
      setLoadingMore(false);
    }
  }, [cursor, filter, hasMore, loading, loadingMore]);

  useEffect(() => {
    if (tasks.length === 0) {
      if (selectedId !== null) setSelectedId(null);
      return;
    }
    if (!tasks.some((task) => task.id === selectedId)) {
      setSelectedId(tasks[0].id);
    }
  }, [selectedId, tasks]);

  const selectedTask = tasks.find((task) => task.id === selectedId) ?? null;

  async function addTask(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const title = newTitle.trim();
    if (!title || creating) return;
    setCreating(true);
    setError(null);
    try {
      const created = await createTask({ title, note: null });
      setNewTitle("");
      setSelectedId(created.id);
      if (filter === "open" || filter === "all") {
        setTasks((current) => [created, ...current.filter((task) => task.id !== created.id)]);
        setCounts(await countTasks());
      } else {
        // 新建的待办一定是进行中：切到该标签页，重载后它会排在最前。
        setFilter("open");
      }
      notifyTasksChanged();
    } catch (createError) {
      setError(createError instanceof Error ? createError.message : "创建待办失败，请重试。");
    } finally {
      setCreating(false);
    }
  }

  /** 状态动作共用：成功后按当前筛选决定就地更新还是移除。 */
  async function changeStatus(action: "done" | "cancelled" | "open") {
    if (!selectedTask || mutating) return;
    setMutating(true);
    setError(null);
    try {
      const updated =
        action === "done"
          ? await completeTask(selectedTask.id)
          : action === "cancelled"
            ? await cancelTask(selectedTask.id)
            : await reopenTask(selectedTask.id);
      const stillMatches = filter === "all" || updated.status === filter;
      setTasks((current) =>
        stillMatches
          ? current.map((task) => (task.id === updated.id ? updated : task))
          : current.filter((task) => task.id !== updated.id),
      );
      setCounts(await countTasks());
      notifyTasksChanged();
    } catch (updateError) {
      setError(updateError instanceof Error ? updateError.message : "更新状态失败，请重试。");
    } finally {
      setMutating(false);
    }
  }

  async function saveTask(title: string, note: string | null) {
    if (!selectedTask) return;
    setError(null);
    try {
      const updated = await updateTask({ id: selectedTask.id, title, note });
      setTasks((current) => current.map((task) => (task.id === updated.id ? updated : task)));
      notifyTasksChanged();
    } catch (updateError) {
      setError(updateError instanceof Error ? updateError.message : "保存失败，请重试。");
      throw updateError;
    }
  }

  const tabs = [
    { id: "open", label: "进行中", count: counts.open },
    { id: "done", label: "已完成", count: counts.done },
    { id: "cancelled", label: "已取消", count: counts.cancelled },
    { id: "all", label: "全部", count: counts.all },
  ];

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <header className="px-5 pt-4 sm:px-7">
        <div className="flex flex-wrap items-baseline justify-between gap-3">
          <div className="flex items-baseline gap-3">
            <h2 className="text-lg font-bold tracking-tight">待办</h2>
            <span className="font-mono text-xs text-muted-foreground">
              进行中 {counts.open} / 共 {counts.all}
            </span>
          </div>
          <Button variant="ghost" size="sm" onClick={() => void load()}>
            刷新
          </Button>
        </div>

        <form className="mt-3 flex max-w-xl gap-2" onSubmit={(event) => void addTask(event)}>
          <Input
            className="h-8 flex-1"
            value={newTitle}
            maxLength={512}
            placeholder="新建待办：标题，回车添加"
            onChange={(event) => setNewTitle(event.target.value)}
          />
          <Button size="sm" type="submit" disabled={!newTitle.trim() || creating}>
            {creating ? "添加中…" : "添加"}
          </Button>
        </form>

        <div className="mt-3">
          <Tabs items={tabs} value={filter} onChange={(id) => setFilter(id as StatusFilter)} />
        </div>
      </header>

      <div className="grid min-h-0 flex-1 lg:grid-cols-[minmax(18rem,0.9fr)_minmax(22rem,1.1fr)]">
        <section
          className="min-h-72 overflow-y-auto border-b border-border lg:border-r lg:border-b-0"
          aria-label="待办列表"
          aria-busy={loading}
        >
          {loading ? (
            <div className="divide-y divide-border" role="status" aria-label="正在加载">
              {Array.from({ length: 5 }).map((_, index) => (
                <div key={index} className="space-y-2 px-4 py-3" aria-hidden>
                  <div className="h-3.5 w-2/3 animate-pulse rounded bg-muted" />
                  <div className="h-3 w-1/3 animate-pulse rounded bg-muted/70" />
                </div>
              ))}
            </div>
          ) : null}

          {!loading && error ? (
            <div className="m-4 callout callout-error px-4 py-3 text-sm text-destructive">
              <p>{error}</p>
              <button
                className="mt-2 text-xs font-medium underline"
                type="button"
                onClick={() => void load()}
              >
                重新加载
              </button>
            </div>
          ) : null}

          {!loading && !error && tasks.length === 0 ? (
            <div className="px-4 py-10 text-center">
              <p className="text-sm text-foreground">{emptyHints[filter].title}</p>
              <p className="mt-1 text-sm text-muted-foreground">{emptyHints[filter].hint}</p>
            </div>
          ) : null}

          <div className="divide-y divide-border">
            {tasks.map((task) => (
              <ListItem
                key={task.id}
                active={task.id === selectedId}
                flush
                className="px-4 py-3"
                aria-current={task.id === selectedId ? "true" : undefined}
                onClick={() => setSelectedId(task.id)}
              >
                <span className="flex items-start gap-2">
                  <ListItemTitle clamp={2} className={task.status === "open" ? "" : "opacity-70"}>
                    {task.title}
                  </ListItemTitle>
                  {task.status !== "open" ? (
                    <Badge tone={taskStatusTones[task.status]} className="mt-0.5">
                      {taskStatusLabels[task.status]}
                    </Badge>
                  ) : null}
                </span>
                <p className="line-clamp-1 text-sm text-muted-foreground">
                  {taskNotePreview(task)}
                </p>
                <ListItemMeta>
                  <span>{describeOrigin(task)}</span>
                  <time className="ml-auto" dateTime={task.updatedAt}>
                    {formatRelative(task.updatedAt)}
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
              ) : tasks.length > 0 ? (
                <span className="font-mono text-xs text-muted-foreground">已显示全部</span>
              ) : null}
            </div>
          ) : null}
        </section>

        <TaskDetail
          task={selectedTask}
          mutating={mutating}
          onSave={saveTask}
          onComplete={() => void changeStatus("done")}
          onCancel={() => void changeStatus("cancelled")}
          onReopen={() => void changeStatus("open")}
          onOpenMessage={onOpenMessage}
        />
      </div>
    </div>
  );
}

export default TasksPage;
