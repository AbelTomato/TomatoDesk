import { useCallback, useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { Button, Row } from "../components/ui";
import CapturesPage from "../features/captures/CapturesPage";
import InboxPage from "../features/inbox/InboxPage";
import RepoDetail from "../features/repos/RepoDetail";
import ReposPage from "../features/repos/ReposPage";
import SearchPage from "../features/search/SearchPage";
import SettingsPage from "../features/settings/SettingsPage";
import TasksPage from "../features/tasks/TasksPage";
import UpdateToast from "../features/update/UpdateToast";
import {
  countCaptures,
  countInboxMessages,
  countTasks,
  getMigrationNotice,
  openCapture,
  type CaptureCounts,
  type InboxCounts,
  type MigrationNotice,
  type TaskCounts,
} from "../lib/api";
import { onCapturesChanged, onInboxChanged, onTasksChanged } from "../lib/events";

type View = "inbox" | "tasks" | "captures" | "repos" | "search" | "settings";

const emptyCounts: CaptureCounts = { all: 0, active: 0, archived: 0 };
const emptyTaskCounts: TaskCounts = { open: 0, done: 0, cancelled: 0, all: 0 };
const emptyInboxCounts: InboxCounts = {
  all: 0,
  pending: 0,
  task: 0,
  dismissed: 0,
  unread: 0,
};

const navItems: Array<{ id: View; label: string }> = [
  { id: "inbox", label: "收件箱" },
  { id: "tasks", label: "待办" },
  { id: "captures", label: "随手记" },
  { id: "repos", label: "仓库" },
  { id: "search", label: "搜索" },
  { id: "settings", label: "设置" },
];

/** `?view=captures&repo=<id>&thread=<id>` 只用于逐页走查与分享定位，缺省是随手记页。 */
function readInitialRoute(): {
  view: View;
  repoId: string | null;
  threadId: string | null;
  inboxMessageId: string | null;
} {
  const params = new URLSearchParams(window.location.search);
  const requested = params.get("view");
  const view = navItems.some((item) => item.id === requested) ? (requested as View) : "captures";
  return {
    view,
    repoId: view === "repos" ? params.get("repo") : null,
    threadId: view === "repos" ? params.get("thread") : null,
    inboxMessageId: view === "inbox" ? params.get("message") : null,
  };
}

function AppShell() {
  const [route] = useState(readInitialRoute);
  const [view, setView] = useState<View>(route.view);
  const [repoId, setRepoId] = useState<string | null>(route.repoId);
  /** 只作为详情页的初值：真正的开关在 RepoDetail 内部，返回列表时由这里清空。 */
  const [threadId, setThreadId] = useState<string | null>(route.threadId);
  const [inboxMessageId, setInboxMessageId] = useState<string | null>(route.inboxMessageId);
  const [counts, setCounts] = useState<CaptureCounts>(emptyCounts);
  const [taskCounts, setTaskCounts] = useState<TaskCounts>(emptyTaskCounts);
  const [inboxCounts, setInboxCounts] = useState<InboxCounts>(emptyInboxCounts);
  const [notice, setNotice] = useState<MigrationNotice | null>(null);
  const [noticeDismissed, setNoticeDismissed] = useState(false);

  // 三个角标互不影响：任意一个失败时保留上一次的数字。
  const refreshCounts = useCallback(async () => {
    const [captures, tasks, inbox] = await Promise.allSettled([
      countCaptures(),
      countTasks(),
      countInboxMessages(),
    ]);
    if (captures.status === "fulfilled") setCounts(captures.value);
    if (tasks.status === "fulfilled") setTaskCounts(tasks.value);
    if (inbox.status === "fulfilled") setInboxCounts(inbox.value);
  }, []);

  useEffect(() => {
    void refreshCounts();
    const stopCaptures = onCapturesChanged(() => void refreshCounts());
    const stopTasks = onTasksChanged(() => void refreshCounts());
    const stopInbox = onInboxChanged(() => void refreshCounts());
    return () => {
      stopCaptures();
      stopTasks();
      stopInbox();
    };
  }, [refreshCounts]);

  useEffect(() => {
    if (!isTauri()) return;
    void getMigrationNotice()
      .then(setNotice)
      .catch(() => setNotice(null));
  }, []);

  // 托盘与后台同步都会改动数据，统一在这里刷新角标。
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    const currentWindow = getCurrentWindow();
    const stops: Array<() => void> = [];
    for (const event of ["github-sync-updated", "inbox-sync-updated", "quick-capture-completed"]) {
      void currentWindow
        .listen(event, () => {
          if (!disposed) void refreshCounts();
        })
        .then((stop) => {
          if (disposed) stop();
          else stops.push(stop);
        });
    }
    return () => {
      disposed = true;
      stops.forEach((stop) => stop());
    };
  }, [refreshCounts]);

  function navigate(next: View) {
    setView(next);
    setInboxMessageId(null);
    if (next !== "repos") {
      setRepoId(null);
      setThreadId(null);
    }
  }

  /** 收件箱里的通知命中本地线程时，直接落到仓库详情的线程页。 */
  function openThread(nextRepoId: string, nextThreadId: string) {
    setRepoId(nextRepoId);
    setThreadId(nextThreadId);
    setView("repos");
  }

  /** 待办「查看原消息」：回到收件箱并定位到那条消息。 */
  function openInboxMessage(id: string) {
    setInboxMessageId(id);
    setView("inbox");
  }

  function openRepo(id: string) {
    setRepoId(id);
    setThreadId(null);
  }

  function closeRepo() {
    setRepoId(null);
    setThreadId(null);
  }

  const showMigrationNotice = Boolean(notice?.migratedTo) && !noticeDismissed;

  return (
    <main className="flex min-h-screen flex-col bg-background text-foreground lg:h-screen lg:overflow-hidden lg:flex-row">
      <aside className="flex w-full shrink-0 flex-col border-b border-border px-4 py-4 lg:h-full lg:w-60 lg:overflow-y-auto lg:border-r lg:border-b-0 lg:px-5 lg:py-6">
        <div className="flex items-baseline justify-between gap-4 lg:block">
          <h1 className="text-base font-bold tracking-tight">TomatoDesk</h1>
        </div>

        <Button className="mt-5 w-full" onClick={() => void openCapture()}>
          随手记一条
        </Button>

        <nav className="mt-5 grid grid-cols-2 gap-1 lg:grid-cols-1" aria-label="主导航">
          {navItems.map((item) => (
            <Row
              key={item.id}
              active={view === item.id}
              aria-current={view === item.id ? "page" : undefined}
              trailing={
                item.id === "inbox" ? (
                  <span className="font-mono text-xs text-muted-foreground">
                    {inboxCounts.pending}
                  </span>
                ) : item.id === "tasks" ? (
                  <span className="font-mono text-xs text-muted-foreground">{taskCounts.open}</span>
                ) : item.id === "captures" ? (
                  <span className="font-mono text-xs text-muted-foreground">{counts.active}</span>
                ) : null
              }
              onClick={() => navigate(item.id)}
            >
              {item.label}
            </Row>
          ))}
        </nav>

        <div className="mt-auto hidden border-t border-border pt-4 font-mono text-xs leading-5 text-muted-foreground lg:block">
          <p>Ctrl + K 聚焦搜索</p>
          <p>数据只在本机，不上传</p>
        </div>
      </aside>

      <section className="flex min-h-0 min-w-0 flex-1 flex-col lg:overflow-hidden">
        {showMigrationNotice && notice ? (
          <div className="flex flex-wrap items-start justify-between gap-3 callout callout-warning border-b border-border px-5 py-3 text-sm sm:px-7">
            <p className="min-w-0 leading-6">
              数据已更新到新版本，旧版本已自动备份。
              {notice.message ? <span className="ml-1">{notice.message}</span> : null}
            </p>
            <button
              className="shrink-0 text-xs font-medium text-muted-foreground underline hover:text-foreground"
              type="button"
              onClick={() => setNoticeDismissed(true)}
            >
              知道了
            </button>
          </div>
        ) : null}

        <div className="flex min-h-0 flex-1 flex-col">
          {view === "inbox" ? (
            <InboxPage
              onOpenTasks={() => navigate("tasks")}
              onOpenThread={openThread}
              initialMessageId={inboxMessageId}
            />
          ) : null}
          {view === "tasks" ? <TasksPage onOpenMessage={openInboxMessage} /> : null}
          {view === "captures" ? <CapturesPage /> : null}
          {view === "repos" ? (
            repoId ? (
              <RepoDetail repoId={repoId} initialThreadId={threadId} onBack={closeRepo} />
            ) : (
              <ReposPage onOpenRepo={openRepo} onOpenSettings={() => navigate("settings")} />
            )
          ) : null}
          {view === "search" ? <SearchPage /> : null}
          {view === "settings" ? <SettingsPage /> : null}
        </div>
      </section>

      <UpdateToast />
    </main>
  );
}

export default AppShell;
