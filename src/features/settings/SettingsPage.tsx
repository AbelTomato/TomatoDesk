import { useCallback, useEffect, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { Button, Input, cn } from "../../components/ui";
import {
  addGithubRepo,
  deleteEmailAccount,
  deleteGithubToken,
  exportCapturesJson,
  exportCapturesMarkdown,
  getAppInfo,
  getGithubTokenStatus,
  getInboxReadBackStatus,
  listEmailAccounts,
  listGithubRepos,
  listGithubSyncRuns,
  removeGithubRepo,
  retryInboxReadBack,
  saveEmailAccount,
  saveGithubToken,
  syncAllGithubRepos,
  syncGithubRepo,
  testEmailAccount,
  type AppInfo,
  type EmailAccountInput,
  type EmailAccountStatus,
  type EmailTls,
  type GithubRepo,
  type GithubSyncRun,
  type GithubSyncScope,
  type InboxReadBackStatus,
} from "../../lib/api";
import { formatDate } from "../../lib/format";
import {
  MAX_EMAIL_ACCOUNTS,
  canAddAccount,
  defaultPort,
  describeServer,
  suggestServer,
} from "./emailAccounts";

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

/** 表单里的端口保持字符串，用户在输入过程中可以清空，提交时再转数字。 */
interface EmailForm {
  address: string;
  password: string;
  host: string;
  port: string;
  tls: EmailTls;
  mailbox: string;
}

const emptyEmailForm: EmailForm = {
  address: "",
  password: "",
  host: "",
  port: String(defaultPort("implicit")),
  tls: "implicit",
  mailbox: "INBOX",
};

function SettingsPage() {
  const [repos, setRepos] = useState<GithubRepo[]>([]);
  const [runs, setRuns] = useState<GithubSyncRun[]>([]);
  const [tokenConfigured, setTokenConfigured] = useState(false);
  const [readBack, setReadBack] = useState<InboxReadBackStatus>({
    pending: 0,
    blocked: 0,
    lastError: null,
  });
  const [token, setToken] = useState("");
  const [repositoryUrl, setRepositoryUrl] = useState("");
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);
  const [exportMessage, setExportMessage] = useState<string | null>(null);
  const [exportFailed, setExportFailed] = useState(false);

  const [emailAccounts, setEmailAccounts] = useState<EmailAccountStatus[]>([]);
  const [emailForm, setEmailForm] = useState<EmailForm>(emptyEmailForm);
  /** null 表示正在新增；有值表示在编辑这个账户（地址与服务器锁住）。 */
  const [emailEditingId, setEmailEditingId] = useState<string | null>(null);
  const [emailFormOpen, setEmailFormOpen] = useState(false);
  const [emailBusy, setEmailBusy] = useState(false);
  const [emailConfirmId, setEmailConfirmId] = useState<string | null>(null);
  const [emailMessage, setEmailMessage] = useState<string | null>(null);
  const [emailFailed, setEmailFailed] = useState(false);
  /** 主机被手填过后不再跟着地址自动改写。 */
  const emailHostTouched = useRef(false);

  const reload = useCallback(async () => {
    try {
      const [nextRepos, nextRuns, tokenStatus, nextReadBack, nextEmailAccounts] = await Promise.all(
        [
          listGithubRepos(),
          listGithubSyncRuns(),
          getGithubTokenStatus(),
          getInboxReadBackStatus(),
          listEmailAccounts(),
        ],
      );
      setRepos(nextRepos);
      setRuns(nextRuns);
      setTokenConfigured(tokenStatus.configured);
      setReadBack(nextReadBack);
      setEmailAccounts(nextEmailAccounts);
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
    // 后台同步与回写队列都会改动这里的数据，几个事件都触发重载。
    const listener = Promise.all(
      ["github-sync-updated", "inbox-sync-updated", "inbox-read-back-updated"].map((event) =>
        getCurrentWindow().listen(event, () => {
          if (!disposed) void reload();
        }),
      ),
    );
    void listener.then((stops) => {
      if (disposed) stops.forEach((stop) => stop());
    });
    return () => {
      disposed = true;
      void listener.then((stops) => stops.forEach((stop) => stop())).catch(() => {});
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

  async function retryReadBack() {
    if (busy) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const status = await retryInboxReadBack();
      setReadBack(status);
      setNotice(
        status.blocked === 0
          ? "已重新加入回写队列，稍后自动重试。"
          : "仍有记录没有通过，请确认 Token 权限后重试。",
      );
    } catch (retryError) {
      setError(retryError instanceof Error ? retryError.message : "重试已读回写失败。");
    } finally {
      setBusy(false);
    }
  }

  function openAddEmail() {
    setEmailForm(emptyEmailForm);
    setEmailEditingId(null);
    emailHostTouched.current = false;
    setEmailConfirmId(null);
    setEmailFormOpen(true);
    setEmailMessage(null);
    setEmailFailed(false);
  }

  function openEditEmail(account: EmailAccountStatus) {
    setEmailEditingId(account.id);
    setEmailForm({
      address: account.address,
      password: "",
      host: account.host,
      port: String(account.port),
      tls: account.tls,
      mailbox: account.mailbox,
    });
    // 编辑时地址与服务器锁住，不进预填分支。
    emailHostTouched.current = true;
    setEmailConfirmId(null);
    setEmailFormOpen(true);
    setEmailMessage(null);
    setEmailFailed(false);
  }

  function closeEmailForm() {
    setEmailFormOpen(false);
    setEmailEditingId(null);
    setEmailMessage(null);
    setEmailFailed(false);
  }

  function updateEmailAddress(value: string) {
    setEmailForm((current) => {
      const next = { ...current, address: value };
      // 只在新增且主机没被手填过时跟着域名预填，避免覆盖用户自己改的服务器。
      if (emailEditingId === null && !emailHostTouched.current) {
        const suggestion = suggestServer(value);
        if (suggestion) {
          next.host = suggestion.host;
          next.port = String(suggestion.port);
        }
      }
      return next;
    });
  }

  function updateEmailTls(tls: EmailTls) {
    setEmailForm((current) => ({
      ...current,
      tls,
      // 端口还是上一个模式的默认值（或为空）时跟着切，手填过的端口不动。
      port:
        current.port === "" || current.port === String(defaultPort(current.tls))
          ? String(defaultPort(tls))
          : current.port,
    }));
  }

  /** 表单 → IPC 入参：授权码留空时传 null，后端回落到已保存的凭据。 */
  function emailInput(): EmailAccountInput {
    // 端口留空交给后端按加密方式补默认值；填了 0 就原样送过去让后端拒绝，
    // 这里不替用户改成默认端口，否则「端口写错」会变成一件看不见的事。
    const portText = emailForm.port.trim();
    const port = portText === "" ? null : Number(portText);
    return {
      address: emailForm.address.trim(),
      password: emailForm.password.trim() || null,
      host: emailForm.host.trim(),
      port: port !== null && Number.isInteger(port) ? port : null,
      tls: emailForm.tls,
      mailbox: emailForm.mailbox.trim() || null,
    };
  }

  const emailAddressReady = emailForm.address.trim().includes("@");
  const emailHostReady = emailForm.host.trim().length > 0;
  // 新增账户必须填授权码；编辑时留空表示沿用已保存的值。
  const emailPasswordReady = emailEditingId !== null || emailForm.password.trim().length > 0;
  const emailSubmittable = emailAddressReady && emailHostReady && emailPasswordReady;

  async function testEmail() {
    if (emailBusy || !emailSubmittable) return;
    setEmailBusy(true);
    setEmailMessage(null);
    setEmailFailed(false);
    try {
      const probe = await testEmailAccount(emailInput());
      setEmailMessage(
        `连接成功：邮箱里共 ${probe.mailboxTotal} 封，UIDVALIDITY ${probe.uidValidity}。`,
      );
    } catch (probeError) {
      setEmailFailed(true);
      setEmailMessage(
        probeError instanceof Error ? probeError.message : "连接失败，请检查地址与授权码。",
      );
    } finally {
      setEmailBusy(false);
    }
  }

  async function submitEmailAccount(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (emailBusy || !emailSubmittable) return;
    setEmailBusy(true);
    setEmailMessage(null);
    setEmailFailed(false);
    try {
      const saved = await saveEmailAccount(emailInput());
      setEmailFormOpen(false);
      setEmailEditingId(null);
      setEmailMessage(`已保存 ${saved.address}，最近 14 天的邮件会自动同步进来。`);
      await reload();
    } catch (saveError) {
      setEmailFailed(true);
      setEmailMessage(
        saveError instanceof Error ? saveError.message : "保存邮箱账户失败，请重试。",
      );
    } finally {
      setEmailBusy(false);
    }
  }

  async function removeEmailAccount(account: EmailAccountStatus) {
    if (emailBusy) return;
    setEmailBusy(true);
    setEmailMessage(null);
    setEmailFailed(false);
    try {
      const remaining = await deleteEmailAccount(account.id);
      setEmailConfirmId(null);
      if (emailEditingId === account.id) {
        setEmailFormOpen(false);
        setEmailEditingId(null);
      }
      setEmailMessage(`已删除 ${account.address}，历史邮件保留在收件箱。`);
      if (remaining.length === 0) {
        setEmailForm(emptyEmailForm);
        emailHostTouched.current = false;
        setEmailFormOpen(true);
      }
      setEmailAccounts(remaining);
    } catch (removeError) {
      setEmailFailed(true);
      setEmailMessage(
        removeError instanceof Error ? removeError.message : "删除邮箱账户失败，请重试。",
      );
    } finally {
      setEmailBusy(false);
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
          GitHub 访问凭据、邮箱账户、关注仓库、同步记录与本地数据导出都集中在这里。
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
              title="邮箱账户"
              description="用授权码或应用专用密码接收邮件；同步是只读的，不会改变服务器上的已读状态。"
            >
              {emailAccounts.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                  还没有邮箱账户。填入地址与授权码后，最近 14 天的邮件会自动进入收件箱。
                </p>
              ) : (
                <div className="divide-y divide-border border-t border-border">
                  {emailAccounts.map((account) => (
                    <div key={account.id} className="py-2.5">
                      <div className="flex flex-wrap items-start justify-between gap-3">
                        <div className="min-w-0">
                          <p className="truncate text-sm">{account.address}</p>
                          <p className="mt-0.5 font-mono text-xs text-muted-foreground">
                            {describeServer(account)} ·{" "}
                            {account.tls === "implicit" ? "SSL/TLS" : "STARTTLS"} ·{" "}
                            {account.mailbox}
                          </p>
                          <p className="mt-0.5 font-mono text-xs text-muted-foreground">
                            最近同步：{formatDate(account.lastSyncedAt)}
                          </p>
                          {account.lastError ? (
                            <p className="mt-1 text-xs leading-5 text-destructive">
                              {account.lastError}
                            </p>
                          ) : null}
                        </div>
                        <div className="flex flex-wrap items-center gap-1">
                          {emailConfirmId === account.id ? (
                            <>
                              <span className="font-mono text-xs text-warning">
                                历史邮件会保留在收件箱
                              </span>
                              <Button
                                variant="destructive"
                                size="sm"
                                disabled={emailBusy}
                                onClick={() => void removeEmailAccount(account)}
                              >
                                确认删除
                              </Button>
                              <Button
                                variant="ghost"
                                size="sm"
                                disabled={emailBusy}
                                onClick={() => setEmailConfirmId(null)}
                              >
                                取消
                              </Button>
                            </>
                          ) : (
                            <>
                              <Button
                                variant="ghost"
                                size="sm"
                                disabled={emailBusy}
                                onClick={() => openEditEmail(account)}
                              >
                                编辑
                              </Button>
                              <Button
                                variant="destructive"
                                size="sm"
                                disabled={emailBusy}
                                onClick={() => {
                                  setEmailConfirmId(account.id);
                                  setEmailMessage(null);
                                }}
                              >
                                删除
                              </Button>
                            </>
                          )}
                        </div>
                      </div>
                    </div>
                  ))}
                </div>
              )}

              {!canAddAccount(emailAccounts) ? (
                <p className="mt-2 text-xs leading-5 text-warning">
                  最多 {MAX_EMAIL_ACCOUNTS} 个账户，删除一个后再添加。
                </p>
              ) : null}

              {emailAccounts.length > 0 && !emailFormOpen ? (
                <div className="mt-3">
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={emailBusy || !canAddAccount(emailAccounts)}
                    onClick={openAddEmail}
                  >
                    添加账户
                  </Button>
                </div>
              ) : null}

              {emailFormOpen || emailAccounts.length === 0 ? (
                <form
                  className="mt-3 space-y-2 border-t border-border pt-3"
                  onSubmit={submitEmailAccount}
                >
                  <div className="flex flex-wrap items-baseline justify-between gap-2">
                    <p className="text-xs font-medium text-muted-foreground">
                      {emailEditingId ? "编辑账户" : "添加账户"}
                    </p>
                    <span className="font-mono text-xs text-muted-foreground">
                      已配置 {emailAccounts.length} / {MAX_EMAIL_ACCOUNTS}
                    </span>
                  </div>
                  <div className="grid gap-2 sm:grid-cols-2">
                    <label className="flex flex-col gap-1">
                      <span className="text-xs text-muted-foreground">邮箱地址</span>
                      <Input
                        type="email"
                        value={emailForm.address}
                        onChange={(event) => updateEmailAddress(event.target.value)}
                        placeholder="you@qq.com"
                        maxLength={320}
                        autoComplete="username"
                        disabled={emailBusy || emailEditingId !== null}
                      />
                    </label>
                    <label className="flex flex-col gap-1">
                      <span className="text-xs text-muted-foreground">授权码 / 应用专用密码</span>
                      <Input
                        type="password"
                        value={emailForm.password}
                        onChange={(event) =>
                          setEmailForm((current) => ({ ...current, password: event.target.value }))
                        }
                        placeholder={emailEditingId ? "留空表示不修改" : "QQ / 163 需使用授权码"}
                        maxLength={512}
                        autoComplete="new-password"
                        disabled={emailBusy}
                      />
                    </label>
                    <label className="flex flex-col gap-1">
                      <span className="text-xs text-muted-foreground">服务器</span>
                      <Input
                        value={emailForm.host}
                        onChange={(event) => {
                          emailHostTouched.current = true;
                          setEmailForm((current) => ({ ...current, host: event.target.value }));
                        }}
                        placeholder="imap.qq.com"
                        maxLength={253}
                        disabled={emailBusy || emailEditingId !== null}
                      />
                    </label>
                    <label className="flex flex-col gap-1">
                      <span className="text-xs text-muted-foreground">端口</span>
                      <Input
                        inputMode="numeric"
                        value={emailForm.port}
                        onChange={(event) =>
                          setEmailForm((current) => ({
                            ...current,
                            port: event.target.value.replace(/\D/g, "").slice(0, 5),
                          }))
                        }
                        placeholder={String(defaultPort(emailForm.tls))}
                        disabled={emailBusy}
                      />
                    </label>
                    <label className="flex flex-col gap-1">
                      <span className="text-xs text-muted-foreground">加密方式</span>
                      <select
                        className="h-8 w-full rounded-lg border border-input bg-transparent px-2.5 text-sm text-foreground outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:cursor-not-allowed disabled:opacity-50"
                        value={emailForm.tls}
                        onChange={(event) => updateEmailTls(event.target.value as EmailTls)}
                        disabled={emailBusy}
                      >
                        <option value="implicit">SSL / TLS</option>
                        <option value="starttls">STARTTLS</option>
                      </select>
                    </label>
                    <label className="flex flex-col gap-1">
                      <span className="text-xs text-muted-foreground">文件夹</span>
                      <Input
                        value={emailForm.mailbox}
                        onChange={(event) =>
                          setEmailForm((current) => ({ ...current, mailbox: event.target.value }))
                        }
                        placeholder="INBOX"
                        maxLength={128}
                        disabled={emailBusy}
                      />
                    </label>
                  </div>
                  <p className="text-xs leading-5 text-muted-foreground">
                    QQ、163 与 Gmail 必须使用授权码或应用专用密码，不能填登录密码；Outlook /
                    Microsoft 365 已关闭基本认证，暂不支持。
                  </p>
                  <div className="flex flex-wrap items-center gap-2">
                    <Button type="submit" disabled={emailBusy || !emailSubmittable}>
                      {emailEditingId ? "保存修改" : "保存账户"}
                    </Button>
                    <Button
                      variant="outline"
                      disabled={emailBusy || !emailSubmittable}
                      onClick={() => void testEmail()}
                    >
                      测试连接
                    </Button>
                    {emailAccounts.length > 0 ? (
                      <Button variant="ghost" disabled={emailBusy} onClick={closeEmailForm}>
                        取消
                      </Button>
                    ) : null}
                  </div>
                  {emailMessage ? (
                    <p
                      className={cn(
                        "font-mono text-xs leading-5 break-words",
                        emailFailed ? "text-destructive" : "text-success",
                      )}
                      role={emailFailed ? "alert" : "status"}
                    >
                      {emailMessage}
                    </p>
                  ) : null}
                </form>
              ) : emailMessage ? (
                <p
                  className={cn(
                    "mt-2 font-mono text-xs leading-5 break-words",
                    emailFailed ? "text-destructive" : "text-success",
                  )}
                  role={emailFailed ? "alert" : "status"}
                >
                  {emailMessage}
                </p>
              ) : null}
            </Section>

            <Section
              title="已读回写"
              description="收件箱里的「已读」会异步同步给 GitHub；瞬时失败按指数退避重试，认证问题会停下来等重新授权。"
              action={
                readBack.blocked > 0 ? (
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={busy}
                    onClick={() => void retryReadBack()}
                  >
                    重试
                  </Button>
                ) : null
              }
            >
              {readBack.pending === 0 && readBack.blocked === 0 ? (
                <p className="text-sm text-muted-foreground">没有等待回写的已读记录。</p>
              ) : (
                <div className="space-y-1">
                  <p className="font-mono text-xs">
                    待回写 {readBack.pending} 条 · 需要重新授权 {readBack.blocked} 条
                  </p>
                  {readBack.lastError ? (
                    <p className="text-xs leading-5 text-destructive">{readBack.lastError}</p>
                  ) : null}
                </div>
              )}
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
