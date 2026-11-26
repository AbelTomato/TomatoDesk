import { useCallback, useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { Badge, Button, ListItem, ListItemMeta, ListItemTitle } from "../../components/ui";
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
      <header className="border-b border-border px-5 py-4 sm:px-7">
        <div className="flex flex-wrap items-baseline justify-between gap-3">
          <div className="flex items-baseline gap-3">
            <h2 className="text-lg font-bold tracking-tight">仓库</h2>
            <span className="font-mono text-xs text-muted-foreground">
              {loading ? "加载中…" : `${repos.length} 个仓库`}
            </span>
          </div>
          <div className="flex items-center gap-2">
            <Button variant="ghost" size="sm" onClick={() => void load()}>
              刷新
            </Button>
            <Button
              size="sm"
              disabled={syncing || repos.length === 0}
              onClick={() => void syncAll()}
            >
              {syncing ? "同步中…" : "同步全部"}
            </Button>
          </div>
        </div>
        <p className="mt-1 text-sm text-muted-foreground">
          按仓库查看 Pull Request、Issue、评审、Release 与相关随手记。
        </p>
      </header>

      {error ? (
        <p className="callout callout-error border-b border-border px-5 py-2.5 text-sm text-destructive sm:px-7">
          {error}
        </p>
      ) : null}
      {notice ? (
        <p className="border-b border-border callout callout-success px-5 py-2.5 text-sm text-success sm:px-7">
          {notice}
        </p>
      ) : null}

      <div className="min-h-0 flex-1 overflow-y-auto">
        {loading ? (
          <div className="divide-y divide-border" role="status" aria-label="正在加载仓库">
            {Array.from({ length: 4 }).map((_, index) => (
              <div key={index} className="space-y-2 px-5 py-4 sm:px-7" aria-hidden>
                <div className="h-3.5 w-1/4 animate-pulse rounded bg-muted" />
                <div className="h-3 w-2/3 animate-pulse rounded bg-muted/70" />
              </div>
            ))}
          </div>
        ) : null}

        {!loading && repos.length === 0 ? (
          <div className="px-5 py-12 text-center sm:px-7">
            <p className="text-sm">还没有关注的仓库</p>
            <p className="mx-auto mt-1 max-w-md text-sm leading-6 text-muted-foreground">
              先在设置页保存 GitHub Token 并添加仓库，同步后会在这里按仓库组织内容。
            </p>
            <Button variant="outline" size="sm" className="mt-4" onClick={onOpenSettings}>
              去设置
            </Button>
          </div>
        ) : null}

        <div className="divide-y divide-border">
          {repos.map((repo) => (
            <ListItem
              key={repo.id}
              flush
              className="px-5 py-3.5 sm:px-7"
              onClick={() => onOpenRepo(repo.id)}
            >
              <span className="flex flex-wrap items-center gap-2">
                <ListItemTitle clamp={1}>
                  <span className="text-muted-foreground">{repo.owner}/</span>
                  {repo.name}
                </ListItemTitle>
                {repo.isArchived ? <Badge>archived</Badge> : null}
                {repo.isPrivate ? <Badge>private</Badge> : null}
                {repo.language ? (
                  <span className="font-mono text-xs text-muted-foreground">{repo.language}</span>
                ) : null}
                {repo.stars !== null ? (
                  <span className="font-mono text-xs text-muted-foreground">
                    ★ {formatCount(repo.stars)}
                  </span>
                ) : null}
              </span>
              <p className="line-clamp-2 text-sm leading-6 text-muted-foreground">
                {repo.description ?? "还没有描述，同步后会从仓库元信息与 README 生成。"}
              </p>
              <ListItemMeta>
                <span>PR {repo.openPullRequests}</span>
                <span>Issue {repo.openIssues}</span>
                <span>评审 {repo.reviews}</span>
                <span>Release {repo.releases}</span>
                <span>随手记 {repo.captures}</span>
                <span className="ml-auto">同步于 {formatRelative(repo.lastSyncedAt)}</span>
              </ListItemMeta>
            </ListItem>
          ))}
        </div>
      </div>
    </div>
  );
}

export default ReposPage;
