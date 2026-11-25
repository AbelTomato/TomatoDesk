import { useCallback, useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { listRepos, syncAllGithubRepos, type RepoSummary } from "../../lib/api";
import { formatCount, formatRelative } from "../../lib/format";

interface ReposPageProps {
  onOpenRepo: (id: string) => void;
  onOpenSettings: () => void;
}

function ReposPage({ onOpenRepo, onOpenSettings }: ReposPageProps) {
  const [repos, setRepos] = useState<RepoSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [syncing, setSyncing] = useState(false);

  const load = useCallback(async () => {
    setError(null);
    try {
      setRepos(await listRepos());
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : "加载仓库失败，请重试。");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    const listener = getCurrentWindow().listen("github-sync-updated", () => {
      if (!disposed) void load();
    });
    void listener.then((stop) => {
      if (disposed) stop();
    });
    return () => {
      disposed = true;
      void listener.then((stop) => stop()).catch(() => {});
    };
  }, [load]);

  async function syncAll() {
    if (syncing) return;
    setSyncing(true);
    setError(null);
    setNotice(null);
    try {
      const summaries = await syncAllGithubRepos();
      const changed = summaries.reduce((total, summary) => total + summary.changedCount, 0);
      setNotice(`同步完成，检查 ${summaries.length} 个仓库，${changed} 条内容有更新。`);
    } catch (syncError) {
      setError(syncError instanceof Error ? syncError.message : "同步失败，请重试。");
    } finally {
      setSyncing(false);
      void load();
    }
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <header className="border-b border-border bg-card px-5 py-5 sm:px-7">
        <div className="flex flex-wrap items-end justify-between gap-4">
          <div>
            <p className="text-sm text-muted-foreground">GitHub</p>
            <h2 className="mt-1 text-2xl font-bold tracking-tight">仓库</h2>
            <p className="mt-2 text-sm text-muted-foreground">
              按仓库查看 Pull Request、Issue、评审、Release 与自己的条目。
            </p>
          </div>
          <div className="flex gap-2">
            <button
              className="h-8 rounded-lg border border-border px-3 text-sm text-foreground/80 transition hover:bg-muted"
              type="button"
              onClick={() => void load()}
            >
              刷新
            </button>
            <button
              className="h-8 rounded-lg bg-primary px-3 text-sm font-medium text-primary-foreground transition hover:bg-primary/90 disabled:opacity-50"
              type="button"
              disabled={syncing || repos.length === 0}
              onClick={() => void syncAll()}
            >
              {syncing ? "同步中…" : "同步全部"}
            </button>
          </div>
        </div>
        {error ? (
          <p className="mt-4 rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
            {error}
          </p>
        ) : null}
        {notice ? (
          <p className="mt-4 rounded-md border border-success/30 bg-success/10 px-3 py-2 text-sm text-success">
            {notice}
          </p>
        ) : null}
      </header>

      <div className="min-h-0 flex-1 bg-background px-5 py-5 sm:px-7">
        {loading ? (
          <div className="space-y-2" role="status" aria-label="正在加载仓库">
            {Array.from({ length: 4 }).map((_, index) => (
              <div key={index} className="rounded-lg border border-border bg-card p-4" aria-hidden>
                <div className="h-4 w-1/3 animate-pulse rounded bg-muted" />
                <div className="mt-2 h-3 w-2/3 animate-pulse rounded bg-muted/70" />
              </div>
            ))}
          </div>
        ) : null}

        {!loading && repos.length === 0 ? (
          <div className="rounded-xl border border-dashed border-border bg-card p-10 text-center">
            <p className="text-sm font-medium text-foreground/80">还没有关注的仓库</p>
            <p className="mt-2 text-sm leading-6 text-muted-foreground">
              先在设置页保存 GitHub Token 并添加仓库，同步后会在这里按仓库组织内容。
            </p>
            <button
              className="mt-4 h-8 rounded-lg border border-border px-3 text-sm text-foreground/80 transition hover:bg-muted"
              type="button"
              onClick={onOpenSettings}
            >
              去设置
            </button>
          </div>
        ) : null}

        <div className="overflow-hidden rounded-xl border border-border bg-card">
          {repos.map((repo) => (
            <button
              key={repo.id}
              className="flex w-full flex-col gap-2 border-b border-border px-4 py-4 text-left transition last:border-b-0 hover:bg-accent/40"
              type="button"
              onClick={() => onOpenRepo(repo.id)}
            >
              <div className="flex flex-wrap items-center gap-2">
                <span className="text-sm font-medium">
                  <span className="text-muted-foreground">{repo.owner}/</span>
                  {repo.name}
                </span>
                {repo.isArchived ? (
                  <span className="rounded bg-muted px-1.5 py-0.5 font-mono text-[10px] tracking-wider text-muted-foreground uppercase">
                    归档
                  </span>
                ) : null}
                {repo.isPrivate ? (
                  <span className="rounded bg-muted px-1.5 py-0.5 font-mono text-[10px] tracking-wider text-muted-foreground uppercase">
                    私有
                  </span>
                ) : null}
                {repo.language ? (
                  <span className="font-mono text-[11px] text-muted-foreground">
                    {repo.language}
                  </span>
                ) : null}
                {repo.stars !== null ? (
                  <span className="font-mono text-[11px] text-muted-foreground">
                    ★ {formatCount(repo.stars)}
                  </span>
                ) : null}
              </div>
              <p className="line-clamp-2 text-sm leading-6 text-muted-foreground">
                {repo.description ?? "还没有描述，同步后会从仓库元信息与 README 生成。"}
              </p>
              <div className="flex flex-wrap items-center gap-x-4 gap-y-1 font-mono text-[11px] text-muted-foreground">
                <span>PR {repo.openPullRequests}</span>
                <span>Issue {repo.openIssues}</span>
                <span>评审 {repo.reviews}</span>
                <span>Release {repo.releases}</span>
                <span>我的条目 {repo.items}</span>
                <span className="ml-auto">同步于 {formatRelative(repo.lastSyncedAt)}</span>
              </div>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

export default ReposPage;
