import { useCallback, useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

import {
  addGithubRepo,
  deleteGithubToken,
  getGithubTokenStatus,
  listGithubRepos,
  listGithubSyncRuns,
  removeGithubRepo,
  saveGithubToken,
  syncAllGithubRepos,
  syncGithubRepo,
  type GithubRepo,
  type GithubSyncRun,
  type GithubSyncScope,
} from "../../lib/api";

interface GithubPanelProps {
  onSynced: () => void;
}

function formatDate(value: string | null) {
  if (!value) return "尚未同步";
  return new Intl.DateTimeFormat("zh-CN", {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}

function GithubPanel({ onSynced }: GithubPanelProps) {
  const [repos, setRepos] = useState<GithubRepo[]>([]);
  const [runs, setRuns] = useState<GithubSyncRun[]>([]);
  const [tokenConfigured, setTokenConfigured] = useState(false);
  const [token, setToken] = useState("");
  const [repositoryUrl, setRepositoryUrl] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      const [nextRepos, nextRuns, tokenStatus] = await Promise.all([
        listGithubRepos(),
        listGithubSyncRuns(),
        getGithubTokenStatus(),
      ]);
      setRepos(nextRepos);
      setRuns(nextRuns);
      setTokenConfigured(tokenStatus.configured);
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : "加载 GitHub 设置失败。");
    }
  }, []);

  useEffect(() => {
    void reload();
    if (!isTauri()) return;
    let disposed = false;
    const listener = getCurrentWindow().listen("github-sync-updated", () => {
      if (!disposed) {
        void reload();
      }
    });
    void listener.then((stop) => {
      if (disposed) stop();
    });
    return () => {
      disposed = true;
      void listener.then((stop) => stop()).catch(() => {});
    };
  }, [reload]);

  async function submitToken(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!token.trim() || busy) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await saveGithubToken(token);
      setToken("");
      setTokenConfigured(true);
      setNotice("GitHub Token 已安全保存到 Windows 凭据管理器。");
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : "保存 Token 失败。");
    } finally {
      setBusy(false);
    }
  }

  async function clearToken() {
    if (busy) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await deleteGithubToken();
      setTokenConfigured(false);
      setNotice("GitHub Token 已删除。");
    } catch (deleteError) {
      setError(deleteError instanceof Error ? deleteError.message : "删除 Token 失败。");
    } finally {
      setBusy(false);
    }
  }

  async function submitRepository(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!repositoryUrl.trim() || busy) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const created = await addGithubRepo(repositoryUrl);
      setRepos((current) =>
        [...current, created].sort((a, b) =>
          `${a.owner}/${a.name}`.localeCompare(`${b.owner}/${b.name}`),
        ),
      );
      setRepositoryUrl("");
      setNotice(`已添加 ${created.owner}/${created.name}。`);
    } catch (addError) {
      setError(addError instanceof Error ? addError.message : "添加仓库失败。");
    } finally {
      setBusy(false);
    }
  }

  async function deleteRepository(repo: GithubRepo) {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await removeGithubRepo(repo.id);
      setRepos((current) => current.filter((entry) => entry.id !== repo.id));
      setNotice(`已移除 ${repo.owner}/${repo.name}。`);
    } catch (removeError) {
      setError(removeError instanceof Error ? removeError.message : "移除仓库失败。");
    } finally {
      setBusy(false);
    }
  }

  async function sync(repoId?: string, scope: GithubSyncScope = "incremental") {
    if (busy) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      if (repoId) {
        const result = await syncGithubRepo(repoId, scope);
        setNotice(
          result.notModified
            ? "内容没有变化，仓库已是最新。"
            : `${scope === "full" ? "历史加载" : "同步"}完成，新增 ${result.importedCount} 条 GitHub 更新。`,
        );
      } else {
        const results = await syncAllGithubRepos();
        const imported = results.reduce((count, result) => count + result.importedCount, 0);
        setNotice(`同步完成，新增 ${imported} 条 GitHub 更新。`);
      }
      await reload();
    } catch (syncError) {
      setError(syncError instanceof Error ? syncError.message : "GitHub 同步失败，请重试。");
      await reload();
    } finally {
      setBusy(false);
      onSynced();
    }
  }

  return (
    <div className="min-h-full bg-background px-5 py-6 sm:px-7">
      <div className="mx-auto max-w-4xl space-y-6">
        <header className="flex flex-wrap items-end justify-between gap-4">
          <div>
            <p className="text-sm font-medium text-muted-foreground">GitHub</p>
            <h2 className="mt-1 text-2xl font-semibold tracking-tight">仓库同步</h2>
            <p className="mt-2 text-sm text-muted-foreground">
              同步 Releases、Issues 和 Pull Requests 到 Inbox。
            </p>
          </div>
          <button
            className="rounded-md bg-primary px-4 py-2.5 text-sm font-semibold text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
            type="button"
            disabled={busy || !tokenConfigured || repos.length === 0}
            onClick={() => void sync()}
          >
            同步所有仓库
          </button>
        </header>

        {error ? (
          <p className="rounded-md border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
            {error}
          </p>
        ) : null}
        {notice ? (
          <p className="rounded-md border border-success/30 bg-success/10 px-4 py-3 text-sm text-success">
            {notice}
          </p>
        ) : null}

        <section className="rounded-lg border border-border bg-card p-5 sm:p-6">
          <div className="flex flex-wrap items-start justify-between gap-4">
            <div>
              <h3 className="font-semibold">Personal Access Token</h3>
              <p className="mt-1 text-sm text-muted-foreground">
                Fine-grained Token 需授予 Contents、Issues 和 Pull requests 的只读权限。Token 保存在
                Windows 凭据管理器。
              </p>
            </div>
            <span
              className={`rounded-full px-2.5 py-1 text-xs font-semibold ${tokenConfigured ? "bg-success/10 text-success" : "bg-warning/10 text-warning"}`}
            >
              {tokenConfigured ? "已配置" : "未配置"}
            </span>
          </div>
          <form className="mt-4 flex flex-col gap-2 sm:flex-row" onSubmit={submitToken}>
            <input
              className="min-w-0 flex-1 rounded-md border border-border bg-background px-3 py-2.5 text-sm outline-none focus:border-ring focus:ring-2 focus:ring-ring/20"
              type="password"
              autoComplete="new-password"
              value={token}
              onChange={(event) => setToken(event.target.value)}
              placeholder={tokenConfigured ? "输入新 Token 以替换" : "github_pat_…"}
              maxLength={512}
            />
            <button
              className="rounded-md border border-border px-4 py-2.5 text-sm font-semibold text-foreground/80 hover:bg-background disabled:opacity-50"
              type="submit"
              disabled={busy || !token.trim()}
            >
              保存 Token
            </button>
            {tokenConfigured ? (
              <button
                className="rounded-md border border-border px-4 py-2.5 text-sm font-semibold text-destructive hover:bg-destructive/10 disabled:opacity-50"
                type="button"
                disabled={busy}
                onClick={() => void clearToken()}
              >
                删除 Token
              </button>
            ) : null}
          </form>
        </section>

        <section className="rounded-lg border border-border bg-card p-5 sm:p-6">
          <h3 className="font-semibold">关注的仓库</h3>
          <form className="mt-4 flex flex-col gap-2 sm:flex-row" onSubmit={submitRepository}>
            <input
              className="min-w-0 flex-1 rounded-md border border-border bg-background px-3 py-2.5 text-sm outline-none focus:border-ring focus:ring-2 focus:ring-ring/20"
              type="url"
              value={repositoryUrl}
              onChange={(event) => setRepositoryUrl(event.target.value)}
              placeholder="https://github.com/owner/repository"
              maxLength={4096}
            />
            <button
              className="rounded-md bg-primary px-4 py-2.5 text-sm font-semibold text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
              type="submit"
              disabled={busy || !repositoryUrl.trim()}
            >
              添加仓库
            </button>
          </form>
          <div className="mt-4 divide-y divide-border">
            {repos.map((repo) => (
              <div
                key={repo.id}
                className="flex flex-wrap items-center justify-between gap-3 py-4 first:pt-0 last:pb-0"
              >
                <div>
                  <p className="font-medium">
                    {repo.owner}/{repo.name}
                  </p>
                  <p className="mt-1 text-xs text-muted-foreground">
                    最近同步：{formatDate(repo.lastSyncedAt)}
                  </p>
                </div>
                <div className="flex gap-2">
                  <button
                    className="h-8 rounded-lg border border-border px-3 text-sm font-medium text-foreground/80 transition hover:bg-muted disabled:opacity-50"
                    type="button"
                    disabled={busy || !tokenConfigured}
                    onClick={() => void sync(repo.id)}
                  >
                    立即同步
                  </button>
                  <button
                    className="h-8 rounded-lg border border-border px-3 text-sm font-medium text-muted-foreground transition hover:bg-muted hover:text-foreground disabled:opacity-50"
                    type="button"
                    title="拉取全部状态（含已关闭）的历史内容，不受增量游标限制"
                    disabled={busy || !tokenConfigured}
                    onClick={() => void sync(repo.id, "full")}
                  >
                    加载历史
                  </button>
                  <button
                    className="h-8 rounded-lg border border-border px-3 text-sm text-destructive transition hover:bg-destructive/10 disabled:opacity-50"
                    type="button"
                    disabled={busy}
                    onClick={() => void deleteRepository(repo)}
                  >
                    移除
                  </button>
                </div>
              </div>
            ))}
            {repos.length === 0 ? (
              <p className="py-5 text-sm text-muted-foreground">还没有关注的仓库。</p>
            ) : null}
          </div>
        </section>

        <section className="rounded-lg border border-border bg-card p-5 sm:p-6">
          <div className="flex items-center justify-between gap-4">
            <h3 className="font-semibold">最近同步记录</h3>
            <button
              className="text-sm font-medium text-link hover:underline"
              type="button"
              onClick={() => void reload()}
            >
              刷新
            </button>
          </div>
          <div className="mt-3 divide-y divide-border">
            {runs.map((run) => (
              <div key={run.id} className="py-3 first:pt-1 last:pb-1">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <span
                    className={`rounded-full px-2 py-0.5 text-xs font-semibold ${run.status === "succeeded" ? "bg-success/10 text-success" : run.status === "failed" ? "bg-destructive/10 text-destructive" : "bg-warning/10 text-warning"}`}
                  >
                    {run.status === "succeeded"
                      ? "成功"
                      : run.status === "failed"
                        ? "失败"
                        : "进行中"}
                  </span>
                  <time className="text-xs text-muted-foreground" dateTime={run.startedAt}>
                    {formatDate(run.startedAt)}
                  </time>
                </div>
                {run.error ? <p className="mt-2 text-sm text-destructive">{run.error}</p> : null}
              </div>
            ))}
            {runs.length === 0 ? (
              <p className="py-4 text-sm text-muted-foreground">还没有同步记录。</p>
            ) : null}
          </div>
        </section>
      </div>
    </div>
  );
}

export default GithubPanel;
