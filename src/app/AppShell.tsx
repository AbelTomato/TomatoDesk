import { useCallback, useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

import Inbox from "../features/inbox/Inbox";
import RepoDetail from "../features/repos/RepoDetail";
import ReposPage from "../features/repos/ReposPage";
import SearchPage from "../features/search/SearchPage";
import SettingsPage from "../features/settings/SettingsPage";
import UpdateBanner from "../features/update/UpdateBanner";
import {
  countItemsByStatus,
  getMigrationNotice,
  openCapture,
  type ItemStatusCounts,
  type MigrationNotice,
} from "../lib/api";
import { appEnvironment } from "../lib/env";
import { onItemsChanged } from "../lib/events";

type View = "inbox" | "repos" | "search" | "settings";

const emptyCounts: ItemStatusCounts = { all: 0, pending: 0, processed: 0, archived: 0 };

const navItems: Array<{ id: View; label: string }> = [
  { id: "inbox", label: "收集" },
  { id: "repos", label: "仓库" },
  { id: "search", label: "搜索" },
  { id: "settings", label: "设置" },
];

function AppShell() {
  const [view, setView] = useState<View>("inbox");
  const [repoId, setRepoId] = useState<string | null>(null);
  const [counts, setCounts] = useState<ItemStatusCounts>(emptyCounts);
  const [notice, setNotice] = useState<MigrationNotice | null>(null);
  const [noticeDismissed, setNoticeDismissed] = useState(false);

  const refreshCounts = useCallback(async () => {
    try {
      setCounts(await countItemsByStatus());
    } catch {
      // 角标失败不影响主流程，保留上一次的数字。
    }
  }, []);

  useEffect(() => {
    void refreshCounts();
    return onItemsChanged(() => void refreshCounts());
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
    for (const event of ["github-sync-updated", "quick-capture-completed"]) {
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
    if (next !== "repos") setRepoId(null);
  }

  const showMigrationNotice = Boolean(notice?.migratedTo) && !noticeDismissed;

  return (
    <main className="flex min-h-screen bg-background text-foreground">
      <aside className="flex w-full shrink-0 flex-col border-b border-border bg-card px-5 py-5 lg:sticky lg:top-0 lg:h-screen lg:w-64 lg:overflow-y-auto lg:border-r lg:border-b-0 lg:px-6 lg:py-7">
        <div className="flex items-center justify-between gap-4 lg:block">
          <div>
            <p className="text-xs font-medium tracking-wide text-link">TomatoDesk</p>
            <h1 className="mt-1 text-xl font-bold tracking-tight">工作站</h1>
          </div>
          <span className="rounded border border-border px-2 py-0.5 font-mono text-[10px] uppercase tracking-wider text-muted-foreground">
            {appEnvironment}
          </span>
        </div>

        <button
          className="mt-6 flex h-8 items-center justify-center rounded-lg bg-primary px-3 text-sm font-medium text-primary-foreground transition hover:bg-primary/90 active:translate-y-px focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none"
          type="button"
          onClick={() => void openCapture()}
        >
          快速收集
        </button>

        <nav className="mt-7 grid grid-cols-2 gap-2 lg:grid-cols-1" aria-label="主导航">
          {navItems.map((item) => {
            const active = view === item.id;
            return (
              <button
                key={item.id}
                className={`flex items-center justify-between rounded-md px-3 py-2 text-left text-sm transition ${
                  active
                    ? "bg-accent font-medium text-link"
                    : "text-muted-foreground hover:bg-accent hover:text-foreground"
                }`}
                type="button"
                aria-current={active ? "page" : undefined}
                onClick={() => navigate(item.id)}
              >
                <span>{item.label}</span>
                {item.id === "inbox" ? (
                  <span className="font-mono text-xs text-muted-foreground">{counts.pending}</span>
                ) : null}
              </button>
            );
          })}
        </nav>

        <div className="mt-auto hidden border-t border-border pt-5 font-mono text-xs leading-5 text-muted-foreground lg:block">
          <p>Ctrl + K 聚焦搜索</p>
          <p>数据保存在本机 SQLite</p>
        </div>
      </aside>

      <section className="flex min-h-0 min-w-0 flex-1 flex-col">
        <UpdateBanner />
        {showMigrationNotice && notice ? (
          <div className="flex flex-wrap items-start justify-between gap-3 border-b border-warning/30 bg-warning/10 px-5 py-3 text-sm text-warning sm:px-7">
            <p className="min-w-0 leading-6">
              数据库已升级到 v{notice.migratedTo}，旧 GitHub
              条目已清理，请到「仓库」页重新同步一次。
              {notice.backupPath ? (
                <span className="ml-1 break-all font-mono text-[11px] opacity-80">
                  迁移前备份：{notice.backupPath}
                </span>
              ) : null}
            </p>
            <button
              className="shrink-0 text-xs font-medium underline"
              type="button"
              onClick={() => setNoticeDismissed(true)}
            >
              知道了
            </button>
          </div>
        ) : null}

        <div className="flex min-h-0 flex-1 flex-col">
          {view === "inbox" ? <Inbox /> : null}
          {view === "repos" ? (
            repoId ? (
              <RepoDetail repoId={repoId} onBack={() => setRepoId(null)} />
            ) : (
              <ReposPage onOpenRepo={setRepoId} onOpenSettings={() => navigate("settings")} />
            )
          ) : null}
          {view === "search" ? <SearchPage /> : null}
          {view === "settings" ? <SettingsPage /> : null}
        </div>
      </section>
    </main>
  );
}

export default AppShell;
