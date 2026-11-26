import { useCallback, useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { Button, Input, cn } from "../../components/ui";
import {
  addGithubRepo,
  deleteGithubToken,
  exportCapturesJson,
  exportCapturesMarkdown,
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

function Section({
  title,
  description,
  action,
  children,
}: {
  title: string;
  description?: string;
  action?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <section className="py-6">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <h3 className="text-sm font-bold tracking-tight">{title}</h3>
          {description ? (
            <p className="mt-1 text-sm leading-6 text-muted-foreground">{description}</p>
          ) : null}
        </div>
        {action ? <div className="shrink-0">{action}</div> : null}
      </div>
      <div className="mt-3">{children}</div>
    </section>
  );
}

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
      const result =
        format === "json" ? await exportCapturesJson() : await exportCapturesMarkdown();
      if (result) setExportMessage(`已导出 ${result.captureCount} 条随手记。`);
    } catch (exportError) {
      setExportFailed(true);
      setExportMessage(exportError instanceof Error ? exportError.message : "导出失败，请重试。");
    } finally {
      setExporting(false);
    }
  }

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <header className="border-b border-border px-5 py-4 sm:px-7">
        <div className="flex items-baseline gap-3">
          <h2 className="text-lg font-bold tracking-tight">设置</h2>
          <span className="font-mono text-xs text-muted-foreground">
            {info ? `${info.name} ${info.version} · ${info.environment}` : "—"}
          </span>
        </div>
        <p className="mt-1 text-sm text-muted-foreground">
          GitHub 访问凭据、关注仓库、同步记录与本地数据导出都集中在这里。
        </p>
      </header>

      <div className="px-5 py-5 sm:px-7">
        <div className="max-w-3xl">
          {error ? (
            <p className="callout callout-error px-3 py-2 text-sm text-destructive">{error}</p>
          ) : null}
          {notice ? (
            <p className="callout callout-success px-3 py-2 text-sm text-success">{notice}</p>
          ) : null}

          <div className="divide-y divide-border">
            <Section
              title="Personal Access Token"
              description="Fine-grained Token 需授予 Contents、Issues 与 Pull requests 的只读权限，保存在 Windows 凭据管理器。"
              action={
                <span
                  className={cn(
                    "font-mono text-xs",
                    tokenConfigured ? "text-success" : "text-warning",
                  )}
                >
                  {tokenConfigured ? "已配置" : "未配置"}
                </span>
              }
            >
              <form className="flex flex-col gap-2 sm:flex-row" onSubmit={submitToken}>
                <Input
                  className="flex-1"
                  type="password"
                  autoComplete="new-password"
                  value={token}
                  onChange={(event) => setToken(event.target.value)}
                  placeholder={tokenConfigured ? "输入新 Token 以替换" : "github_pat_…"}
                  maxLength={512}
                />
                <Button variant="outline" type="submit" disabled={busy || !token.trim()}>
                  保存 Token
                </Button>
                {tokenConfigured ? (
                  <Button
                    variant="destructive"
                    type="button"
                    disabled={busy}
                    onClick={() => void clearToken()}
                  >
                    删除 Token
                  </Button>
                ) : null}
              </form>
            </Section>

            <Section
              title="关注的仓库"
              description="添加后到「仓库」页同步；同步会拉取元信息、README、Release、PR 与 Issue。"
              action={
                <Button
                  variant="outline"
                  size="sm"
                  disabled={busy || !tokenConfigured || repos.length === 0}
                  onClick={() => void sync()}
                >
                  同步全部
                </Button>
              }
            >
              <form className="flex flex-col gap-2 sm:flex-row" onSubmit={submitRepository}>
                <Input
                  className="flex-1"
                  type="url"
                  value={repositoryUrl}
                  onChange={(event) => setRepositoryUrl(event.target.value)}
                  placeholder="https://github.com/owner/repository"
                  maxLength={4096}
                />
                <Button type="submit" disabled={busy || !repositoryUrl.trim()}>
                  添加仓库
                </Button>
              </form>

              <div className="mt-3 divide-y divide-border border-t border-border">
                {repos.map((repo) => (
                  <div
                    key={repo.id}
                    className="flex flex-wrap items-center justify-between gap-3 py-2.5"
                  >
                    <div className="min-w-0">
                      <p className="text-sm">
                        {repo.owner}/{repo.name}
                      </p>
                      <p className="mt-0.5 font-mono text-xs text-muted-foreground">
                        最近同步：{formatDate(repo.lastSyncedAt)}
                      </p>
                    </div>
                    <div className="flex flex-wrap items-center gap-1">
                      <Button
                        variant="ghost"
                        size="sm"
                        disabled={busy || !tokenConfigured}
                        onClick={() => void sync(repo.id)}
                      >
                        同步
                      </Button>
                      <Button
                        variant="ghost"
                        size="sm"
                        title="拉取全部状态（含已关闭）的历史内容，不受增量游标限制"
                        disabled={busy || !tokenConfigured}
                        onClick={() => void sync(repo.id, "full")}
                      >
                        加载历史
                      </Button>
                      <Button
                        variant="destructive"
                        size="sm"
                        disabled={busy}
                        onClick={() => void deleteRepository(repo)}
                      >
                        移除
                      </Button>
                    </div>
                  </div>
                ))}
                {repos.length === 0 ? (
                  <p className="py-3 text-sm text-muted-foreground">还没有关注的仓库。</p>
                ) : null}
              </div>
            </Section>

            <Section
              title="本地数据"
              description="导出的文件包含全部个人条目、标签与仓库列表，不包含 GitHub 线程正文。"
            >
              <div className="flex flex-wrap gap-2">
                <Button
                  variant="outline"
                  disabled={exporting}
                  onClick={() => void exportData("json")}
                >
                  导出 JSON
                </Button>
                <Button
                  variant="outline"
                  disabled={exporting}
                  onClick={() => void exportData("markdown")}
                >
                  导出 Markdown
                </Button>
              </div>
              {exportMessage ? (
                <p
                  className={cn(
                    "mt-2 font-mono text-xs leading-5 break-words",
                    exportFailed ? "text-destructive" : "text-success",
                  )}
                  role={exportFailed ? "alert" : "status"}
                >
                  {exportMessage}
                </p>
              ) : null}
            </Section>

            <Section title="同步记录">
              <div className="divide-y divide-border border-t border-border">
                {runs.map((run) => (
                  <div key={run.id} className="py-2.5">
                    <div className="flex flex-wrap items-center justify-between gap-2">
                      <span
                        className={cn(
                          "font-mono text-xs",
                          run.status === "succeeded"
                            ? "text-success"
                            : run.status === "failed"
                              ? "text-destructive"
                              : "text-warning",
                        )}
                      >
                        {run.status === "succeeded"
                          ? "成功"
                          : run.status === "failed"
                            ? "失败"
                            : "进行中"}
                      </span>
                      <time className="font-mono text-xs text-muted-foreground">
                        {formatDate(run.startedAt)}
                      </time>
                    </div>
                    {run.error ? (
                      <p className="mt-1 text-sm text-destructive">{run.error}</p>
                    ) : null}
                  </div>
                ))}
                {runs.length === 0 ? (
                  <p className="py-3 text-sm text-muted-foreground">还没有同步记录。</p>
                ) : null}
              </div>
            </Section>

            <Section title="关于">
              <p className="font-mono text-xs text-muted-foreground">
                {info ? `${info.name} ${info.version} · ${info.environment}` : "—"}
              </p>
              <p className="mt-2 text-sm text-muted-foreground">
                设计规范见 <span className="font-mono text-xs">docs/design/design-language.md</span>
                ，开发模式下访问 <span className="font-mono text-xs">?styleguide</span>{" "}
                可查看组件基座。
              </p>
            </Section>
          </div>
        </div>
      </div>
    </div>
  );
}

export default SettingsPage;
