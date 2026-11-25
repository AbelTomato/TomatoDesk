import { useCallback, useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

import {
  addGithubRepo,
  deleteGithubToken,
  exportItemsJson,
  exportItemsMarkdown,
  getAppInfo,
  getGithubTokenStatus,
  listGithubRepos,
  listGithubSyncRuns,
  removeGithubRepo,
  saveGithubToken,
  syncAllGithubRepos,
  syncGithubRepo,
  type AppInfo,
  type GithubRepo,
  type GithubSyncRun,
  type GithubSyncScope,
} from "../../lib/api";
import { formatDate } from "../../lib/format";

function SettingsPage() {
  const [repos, setRepos] = useState<GithubRepo[]>([]);
  const [runs, setRuns] = useState<GithubSyncRun[]>([]);
  const [tokenConfigured, setTokenConfigured] = useState(false);
  const [token, setToken] = useState("");
  const [repositoryUrl, setRepositoryUrl] = useState("");
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);
  const [exportMessage, setExportMessage] = useState<string | null>(null);
  const [exportFailed, setExportFailed] = useState(false);

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
      setError(loadError instanceof Error ? loadError.message : "加载设置失败。");
    }
  }, []);

  useEffect(() => {
    void reload();
    void getAppInfo()
      .then(setInfo)
      .catch(() => setInfo(null));
    if (!isTauri()) return;
    let disposed = false;
    const listener = getCurrentWindow().listen("github-sync-updated", () => {
      if (!disposed) void reload();
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
      setNotice("GitHub Token 已保存到 Windows 凭据管理器。");
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
      setRepositoryUrl("");
      setNotice(`已添加 ${created.owner}/${created.name}，回到「仓库」页同步即可。`);
      await reload();
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
    setNotice(null);
    try {
      await removeGithubRepo(repo.id);
      setNotice(`已移除 ${repo.owner}/${repo.name}，其线程与 Release 一并删除。`);
      await reload();
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
            : `${scope === "full" ? "历史加载" : "同步"}完成，${result.changedCount} 条内容有更新。`,
        );
      } else {
        const results = await syncAllGithubRepos();
        const changed = results.reduce((total, result) => total + result.changedCount, 0);
        setNotice(`同步完成，检查 ${results.length} 个仓库，${changed} 条内容有更新。`);
      }
      await reload();
    } catch (syncError) {
      setError(syncError instanceof Error ? syncError.message : "GitHub 同步失败，请重试。");
      await reload();
    } finally {
      setBusy(false);
    }
  }

  async function exportData(format: "json" | "markdown") {
    if (exporting) return;
    setExporting(true);
    setExportMessage(null);
    setExportFailed(false);
    try {
      const result = format === "json" ? await exportItemsJson() : await exportItemsMarkdown();
      if (result) setExportMessage(`已导出 ${result.itemCount} 条内容。`);
    } catch (exportError) {
      setExportFailed(true);
      setExportMessage(exportError instanceof Error ? exportError.message : "导出失败，请重试。");
    } finally {
      setExporting(false);
    }
  }

  return (
    <div className="min-h-0 flex-1 overflow-y-auto bg-background px-5 py-6 sm:px-7">
      <div className="mx-auto max-w-3xl space-y-6">
        <header>
          <p className="text-sm text-muted-foreground">Settings</p>
          <h2 className="mt-1 text-2xl font-bold tracking-tight">设置</h2>
          <p className="mt-2 text-sm text-muted-foreground">
            GitHub 访问凭据、关注仓库、同步记录与本地数据导出都集中在这里。
          </p>
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

        <section className="rounded-xl border border-border bg-card p-5">
          <div className="flex flex-wrap items-start justify-between gap-4">
            <div>
              <h3 className="font-semibold">Personal Access Token</h3>
              <p className="mt-1 text-sm text-muted-foreground">
                Fine-grained Token 需授予 Contents、Issues 与 Pull requests 的只读权限。Token 保存在
                Windows 凭据管理器。
              </p>
            </div>
            <span
              className={`rounded px-2 py-0.5 font-mono text-[11px] tracking-wider uppercase ${tokenConfigured ? "bg-success/10 text-success" : "bg-warning/10 text-warning"}`}
            >
              {tokenConfigured ? "已配置" : "未配置"}
            </span>
          </div>
          <form className="mt-4 flex flex-col gap-2 sm:flex-row" onSubmit={submitToken}>
            <input
              className="h-8 min-w-0 flex-1 rounded-lg border border-input bg-transparent px-2.5 text-sm outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
              type="password"
              autoComplete="new-password"
              value={token}
              onChange={(event) => setToken(event.target.value)}
              placeholder={tokenConfigured ? "输入新 Token 以替换" : "github_pat_…"}
              maxLength={512}
            />
            <button
              className="h-8 rounded-lg border border-border px-3 text-sm font-medium text-foreground/80 transition hover:bg-muted disabled:opacity-50"
              type="submit"
              disabled={busy || !token.trim()}
            >
              保存 Token
            </button>
            {tokenConfigured ? (
              <button
                className="h-8 rounded-lg border border-border px-3 text-sm text-destructive transition hover:bg-destructive/10 disabled:opacity-50"
                type="button"
                disabled={busy}
                onClick={() => void clearToken()}
              >
                删除 Token
              </button>
            ) : null}
          </form>
        </section>

        <section className="rounded-xl border border-border bg-card p-5">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <h3 className="font-semibold">关注的仓库</h3>
            <button
              className="h-8 rounded-lg border border-border px-3 text-sm text-foreground/80 transition hover:bg-muted disabled:opacity-50"
              type="button"
              disabled={busy || !tokenConfigured || repos.length === 0}
              onClick={() => void sync()}
            >
              同步全部
            </button>
          </div>
          <form className="mt-4 flex flex-col gap-2 sm:flex-row" onSubmit={submitRepository}>
            <input
              className="h-8 min-w-0 flex-1 rounded-lg border border-input bg-transparent px-2.5 text-sm outline-none placeholder:text-muted-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
              type="url"
              value={repositoryUrl}
              onChange={(event) => setRepositoryUrl(event.target.value)}
              placeholder="https://github.com/owner/repository"
              maxLength={4096}
            />
            <button
              className="h-8 rounded-lg bg-primary px-3 text-sm font-medium text-primary-foreground transition hover:bg-primary/90 disabled:opacity-50"
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
                className="flex flex-wrap items-center justify-between gap-3 py-3 first:pt-0 last:pb-0"
              >
                <div className="min-w-0">
                  <p className="text-sm font-medium">
                    {repo.owner}/{repo.name}
                  </p>
                  <p className="mt-1 font-mono text-[11px] text-muted-foreground">
                    最近同步：{formatDate(repo.lastSyncedAt)}
                  </p>
                </div>
                <div className="flex flex-wrap gap-2">
                  <button
                    className="h-7 rounded-md border border-border px-2.5 text-xs text-foreground/80 transition hover:bg-muted disabled:opacity-50"
                    type="button"
                    disabled={busy || !tokenConfigured}
                    onClick={() => void sync(repo.id)}
                  >
                    同步
                  </button>
                  <button
                    className="h-7 rounded-md border border-border px-2.5 text-xs text-muted-foreground transition hover:bg-muted hover:text-foreground disabled:opacity-50"
                    type="button"
                    title="拉取全部状态（含已关闭）的历史内容，不受增量游标限制"
                    disabled={busy || !tokenConfigured}
                    onClick={() => void sync(repo.id, "full")}
                  >
                    加载历史
                  </button>
                  <button
                    className="h-7 rounded-md border border-border px-2.5 text-xs text-destructive transition hover:bg-destructive/10 disabled:opacity-50"
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
              <p className="py-4 text-sm text-muted-foreground">还没有关注的仓库。</p>
            ) : null}
          </div>
        </section>

        <section className="rounded-xl border border-border bg-card p-5">
          <h3 className="font-semibold">本地数据</h3>
          <p className="mt-1 text-sm text-muted-foreground">
            导出的文件包含全部个人条目、标签与仓库列表，不包含 GitHub 线程正文。
          </p>
          <div className="mt-3 flex flex-wrap gap-2">
            <button
              className="h-8 rounded-lg border border-border px-3 text-sm text-foreground/80 transition hover:bg-muted disabled:opacity-50"
              type="button"
              disabled={exporting}
              onClick={() => void exportData("json")}
            >
              导出 JSON
            </button>
            <button
              className="h-8 rounded-lg border border-border px-3 text-sm text-foreground/80 transition hover:bg-muted disabled:opacity-50"
              type="button"
              disabled={exporting}
              onClick={() => void exportData("markdown")}
            >
              导出 Markdown
            </button>
          </div>
          {exportMessage ? (
            <p
              className={`mt-2 text-xs leading-5 break-words ${exportFailed ? "text-destructive" : "text-success"}`}
              role={exportFailed ? "alert" : "status"}
            >
              {exportMessage}
            </p>
          ) : null}
        </section>

        <section className="rounded-xl border border-border bg-card p-5">
          <h3 className="font-semibold">同步记录</h3>
          <div className="mt-3 divide-y divide-border">
            {runs.map((run) => (
              <div key={run.id} className="py-3 first:pt-1 last:pb-1">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <span
                    className={`rounded px-1.5 py-0.5 font-mono text-[10px] tracking-wider uppercase ${run.status === "succeeded" ? "bg-success/10 text-success" : run.status === "failed" ? "bg-destructive/10 text-destructive" : "bg-warning/10 text-warning"}`}
                  >
                    {run.status === "succeeded"
                      ? "成功"
                      : run.status === "failed"
                        ? "失败"
                        : "进行中"}
                  </span>
                  <time className="font-mono text-[11px] text-muted-foreground">
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

        <section className="rounded-xl border border-border bg-card p-5">
          <h3 className="font-semibold">关于</h3>
          <p className="mt-2 font-mono text-xs text-muted-foreground">
            {info ? `${info.name} ${info.version} · ${info.environment}` : "—"}
          </p>
        </section>
      </div>
    </div>
  );
}

export default SettingsPage;
