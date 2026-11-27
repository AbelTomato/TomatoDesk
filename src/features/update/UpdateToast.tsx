import { useCallback, useEffect, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { Update } from "@tauri-apps/plugin-updater";

import { Button } from "../../components/ui";
import { notifyUpdateAvailable } from "../../lib/api";
import {
  clearSkippedVersion,
  describeUpdateError,
  findUpdate,
  formatBytes,
  readNotifiedVersion,
  readSkippedVersion,
  rememberNotifiedVersion,
  rememberSkippedVersion,
} from "../../lib/update";

type Phase =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "ready"
  | "installing"
  | "current"
  | "dev-skip"
  | "error";

/** 收起后保留的小提示对应哪个阶段，点击可重新展开。 */
type CollapsedPhase = "available" | "ready";

const CURRENT_HINT_MS = 4000;

function UpdateToast() {
  const [phase, setPhase] = useState<Phase>("idle");
  const [collapsed, setCollapsed] = useState<CollapsedPhase | null>(null);
  const [version, setVersion] = useState("");
  const [currentVersion, setCurrentVersion] = useState("");
  const [notes, setNotes] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [percent, setPercent] = useState(0);
  const [received, setReceived] = useState(0);
  const [total, setTotal] = useState(0);

  const updateRef = useRef<Update | null>(null);
  const aliveRef = useRef(true);
  const hintTimerRef = useRef<number | null>(null);

  const clearHintTimer = useCallback(() => {
    if (hintTimerRef.current !== null) {
      window.clearTimeout(hintTimerRef.current);
      hintTimerRef.current = null;
    }
  }, []);

  const releaseUpdate = useCallback(() => {
    const pending = updateRef.current;
    updateRef.current = null;
    if (pending) void pending.close().catch(() => undefined);
  }, []);

  useEffect(() => {
    aliveRef.current = true;
    return () => {
      aliveRef.current = false;
      if (hintTimerRef.current !== null) window.clearTimeout(hintTimerRef.current);
      const pending = updateRef.current;
      updateRef.current = null;
      if (pending) void pending.close().catch(() => undefined);
    };
  }, []);

  const runCheck = useCallback(
    async (manual: boolean) => {
      if (!isTauri()) return;
      // 开发构建不注册 updater 插件：自动检查直接跳过，手动检查给一句说明。
      if (import.meta.env.DEV) {
        if (manual) {
          setCollapsed(null);
          setMessage(null);
          setPhase("dev-skip");
        }
        return;
      }
      // 手里已经有一个待处理的更新时，后台不必再查；手动检查仍然照查。
      if (!manual && updateRef.current) return;

      clearHintTimer();
      if (manual) clearSkippedVersion();
      releaseUpdate();
      setCollapsed(null);
      setMessage(null);
      setPhase(manual ? "checking" : "idle");
      try {
        const next = await findUpdate();
        if (!aliveRef.current) {
          if (next) void next.close().catch(() => undefined);
          return;
        }
        if (!next) {
          if (manual) {
            setPhase("current");
            hintTimerRef.current = window.setTimeout(() => setPhase("idle"), CURRENT_HINT_MS);
          } else {
            // 后台检查没有更新时保持安静，不留任何提示。
            setPhase("idle");
          }
          return;
        }
        if (!manual && readSkippedVersion() === next.version) {
          void next.close().catch(() => undefined);
          setPhase("idle");
          return;
        }
        if (!manual && readNotifiedVersion() === next.version) {
          // 同一版本已经告知过：不再发系统通知，也不把收起态重新展开。
          void next.close().catch(() => undefined);
          setPhase("idle");
          return;
        }
        updateRef.current = next;
        setVersion(next.version);
        setCurrentVersion(next.currentVersion);
        setNotes(next.body?.trim() || null);
        setPercent(0);
        setReceived(0);
        setTotal(0);
        rememberNotifiedVersion(next.version);
        if (manual) {
          setPhase("available");
          return;
        }
        // 后台发现新版本：窗口可见就用右下角卡片；隐藏时改发系统通知，
        // 同时留着小胶囊，下次打开主窗口还能找到。
        const visible = await getCurrentWindow()
          .isVisible()
          .catch(() => true);
        if (!aliveRef.current) return;
        if (!visible) {
          void notifyUpdateAvailable(next.version).catch(() => undefined);
          setCollapsed("available");
          setPhase("idle");
          return;
        }
        setPhase("available");
      } catch (error) {
        if (!aliveRef.current) return;
        if (manual) {
          setMessage(describeUpdateError(error));
          setPhase("error");
        } else {
          // 后台检查失败不打扰用户，下次启动或手动检查再试。
          setPhase("idle");
        }
      }
    },
    [clearHintTimer, releaseUpdate],
  );

  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    const stops: Array<() => void> = [];
    // 「何时查」由 Rust 的周期触发器决定，这里只接住到点提醒；
    // 托盘菜单仍然可以随时手动检查。
    const subscribe = (event: string, manual: boolean) => {
      void listen(event, () => void runCheck(manual)).then((stop) => {
        if (disposed) stop();
        else stops.push(stop);
      });
    };
    subscribe("update-check-due", false);
    subscribe("check-updates-requested", true);
    return () => {
      disposed = true;
      stops.forEach((stop) => stop());
    };
  }, [runCheck]);

  const startDownload = useCallback(async () => {
    const update = updateRef.current;
    if (!update) return;
    setPhase("downloading");
    setPercent(0);
    setReceived(0);
    setTotal(0);
    setMessage(null);
    let contentLength = 0;
    let downloaded = 0;
    try {
      await update.download(
        (event) => {
          if (!aliveRef.current) return;
          if (event.event === "Started") {
            contentLength = event.data.contentLength ?? 0;
            setTotal(contentLength);
          } else if (event.event === "Progress") {
            downloaded += event.data.chunkLength;
            setReceived(downloaded);
            if (contentLength > 0) {
              setPercent(Math.min(100, Math.round((downloaded / contentLength) * 100)));
            }
          }
        },
        { timeout: 300000 },
      );
      if (!aliveRef.current) return;
      setPercent(100);
      setPhase("ready");
    } catch (error) {
      if (!aliveRef.current) return;
      setMessage(describeUpdateError(error));
      setPhase("error");
    }
  }, []);

  const installNow = useCallback(async () => {
    const update = updateRef.current;
    if (!update) return;
    setPhase("installing");
    setMessage(null);
    try {
      await update.install({ restartAfterInstall: true });
      updateRef.current = null;
    } catch (error) {
      if (!aliveRef.current) return;
      setMessage(describeUpdateError(error));
      setPhase("error");
    }
  }, []);

  const skipVersion = useCallback(() => {
    const pending = updateRef.current;
    if (pending) rememberSkippedVersion(pending.version);
    releaseUpdate();
    setCollapsed(null);
    setPhase("idle");
  }, [releaseUpdate]);

  /** 稍后：收起成右下角的小提示，更新对象留着，点一下还能继续。 */
  const defer = useCallback((next: CollapsedPhase) => {
    setCollapsed(next);
    setPhase("idle");
  }, []);

  /** 关闭：彻底放弃这次更新提示，下次检查会重新判断。 */
  const dismiss = useCallback(() => {
    releaseUpdate();
    setCollapsed(null);
    setPhase("idle");
  }, [releaseUpdate]);

  const openCollapsed = useCallback(() => {
    if (!collapsed || !updateRef.current) return;
    setPhase(collapsed);
    setCollapsed(null);
  }, [collapsed]);

  if (phase === "idle") {
    if (!collapsed) return null;
    return (
      <div className="fixed right-4 bottom-4 z-50">
        <button
          type="button"
          onClick={openCollapsed}
          className="pointer-events-auto flex items-center gap-2 rounded-full border border-border bg-card px-3 py-1.5 text-xs text-foreground shadow-lg transition hover:bg-accent"
        >
          <span aria-hidden className="size-2 rounded-full bg-link" />
          {collapsed === "ready" ? `v${version} 已下载` : `新版本 v${version} 可用`}
          <span className="text-link">查看</span>
        </button>
      </div>
    );
  }

  const tone =
    phase === "error"
      ? "callout-error"
      : phase === "ready" || phase === "current"
        ? "callout-success"
        : "callout-info";

  const title =
    phase === "checking"
      ? "正在检查更新"
      : phase === "available"
        ? `发现新版本 ${version}`
        : phase === "downloading"
          ? `正在下载 ${version}`
          : phase === "ready"
            ? `新版本 ${version} 已下载`
            : phase === "installing"
              ? "正在启动安装程序"
              : phase === "current"
                ? "已是最新版本"
                : phase === "dev-skip"
                  ? "开发构建不检查更新"
                  : "更新失败";

  const hint =
    phase === "checking"
      ? "正在从 GitHub 获取版本信息。"
      : phase === "available"
        ? `当前版本 ${currentVersion}，下载完成后由你决定何时重启。`
        : phase === "downloading"
          ? "下载期间可以继续使用，完成后会提示你重启。"
          : phase === "ready"
            ? "重启并安装后会自动重新打开 TomatoDesk。"
            : phase === "installing"
              ? "应用即将退出，安装完成后会自动重新打开。"
              : phase === "current"
                ? "当前已是最新发布版本。"
                : phase === "dev-skip"
                  ? "只有发布版本会检查 GitHub 上的新版本。"
                  : (message ?? "更新过程出现未知错误。");

  const canClose = phase !== "downloading" && phase !== "installing";

  return (
    <div className="pointer-events-none fixed right-4 bottom-4 z-50 flex w-[min(23rem,calc(100vw-2rem))] flex-col items-end">
      <section
        className={`callout pointer-events-auto w-full rounded-xl border border-border px-4 py-3 shadow-lg ${tone}`}
        aria-live="polite"
        role="status"
      >
        <div className="flex items-start justify-between gap-3">
          <p
            className={`text-sm font-medium ${
              phase === "error"
                ? "text-destructive"
                : phase === "current"
                  ? "text-foreground"
                  : "text-link"
            }`}
          >
            {title}
          </p>
          {canClose ? (
            <button
              type="button"
              aria-label="关闭更新提示"
              className="-mt-1 -mr-1 size-6 shrink-0 rounded-md text-muted-foreground transition hover:bg-accent hover:text-foreground"
              onClick={dismiss}
            >
              ×
            </button>
          ) : null}
        </div>

        <p className="mt-1 text-xs leading-5 text-muted-foreground">{hint}</p>

        {notes && (phase === "available" || phase === "ready") ? (
          <details className="mt-2">
            <summary className="cursor-pointer text-xs font-medium text-foreground">
              更新说明
            </summary>
            <pre className="mt-2 max-h-32 overflow-auto whitespace-pre-wrap rounded-md border border-border bg-background p-2 text-xs leading-5 text-foreground/80">
              {notes}
            </pre>
          </details>
        ) : null}

        {phase === "downloading" ? (
          <div className="mt-2">
            <div className="h-1.5 w-full overflow-hidden rounded-full bg-muted">
              <div
                className="h-full rounded-full bg-primary transition-[width] duration-300"
                style={{ width: `${percent}%` }}
              />
            </div>
            <p className="mt-1 font-mono text-xs text-muted-foreground">
              {total > 0
                ? `${formatBytes(received)} / ${formatBytes(total)} · ${percent}%`
                : `${formatBytes(received)} 已下载`}
            </p>
          </div>
        ) : null}

        <div className="mt-3 flex flex-wrap items-center gap-2">
          {phase === "checking" ? (
            <span className="text-xs text-muted-foreground">请稍候…</span>
          ) : null}
          {phase === "available" ? (
            <>
              <Button size="sm" onClick={() => void startDownload()}>
                下载并安装
              </Button>
              <Button variant="outline" size="sm" onClick={() => defer("available")}>
                稍后
              </Button>
              <Button variant="ghost" size="sm" onClick={skipVersion}>
                跳过此版本
              </Button>
            </>
          ) : null}
          {phase === "downloading" ? (
            <Button variant="outline" size="sm" disabled>
              下载中…
            </Button>
          ) : null}
          {phase === "ready" ? (
            <>
              <Button size="sm" onClick={() => void installNow()}>
                重启并安装
              </Button>
              <Button variant="outline" size="sm" onClick={() => defer("ready")}>
                稍后
              </Button>
            </>
          ) : null}
          {phase === "installing" ? (
            <span className="text-xs text-muted-foreground">请稍候…</span>
          ) : null}
          {phase === "current" ? (
            <Button variant="outline" size="sm" onClick={dismiss}>
              关闭
            </Button>
          ) : null}
          {phase === "dev-skip" ? (
            <Button variant="outline" size="sm" onClick={dismiss}>
              关闭
            </Button>
          ) : null}
          {phase === "error" ? (
            <>
              <Button size="sm" onClick={() => void runCheck(true)}>
                重试
              </Button>
              <Button variant="outline" size="sm" onClick={dismiss}>
                关闭
              </Button>
            </>
          ) : null}
        </div>
      </section>
    </div>
  );
}

export default UpdateToast;
