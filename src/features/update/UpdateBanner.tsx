import { useCallback, useEffect, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Update } from "@tauri-apps/plugin-updater";

import {
  clearSkippedVersion,
  describeUpdateError,
  findUpdate,
  formatBytes,
  readSkippedVersion,
  rememberSkippedVersion,
} from "../../lib/update";

type Phase =
  "idle" | "checking" | "available" | "downloading" | "ready" | "installing" | "current" | "error";

const AUTO_CHECK_DELAY_MS = 8000;
const CURRENT_HINT_MS = 4000;

function UpdateBanner() {
  const [phase, setPhase] = useState<Phase>("idle");
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
      if (hintTimerRef.current !== null) {
        window.clearTimeout(hintTimerRef.current);
        hintTimerRef.current = null;
      }
      if (manual) clearSkippedVersion();
      releaseUpdate();
      setPhase("checking");
      setMessage(null);
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
            setPhase("idle");
          }
          return;
        }
        if (!manual && readSkippedVersion() === next.version) {
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
        setPhase("available");
      } catch (error) {
        if (!aliveRef.current) return;
        if (manual) {
          setMessage(describeUpdateError(error));
          setPhase("error");
        } else {
          setPhase("idle");
        }
      }
    },
    [releaseUpdate],
  );

  useEffect(() => {
    if (!isTauri()) return;
    const timer = window.setTimeout(() => void runCheck(false), AUTO_CHECK_DELAY_MS);
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen("check-updates-requested", () => void runCheck(true)).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    });
    return () => {
      disposed = true;
      window.clearTimeout(timer);
      if (unlisten) unlisten();
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
    setPhase("idle");
  }, [releaseUpdate]);

  const dismiss = useCallback(() => {
    releaseUpdate();
    setPhase("idle");
  }, [releaseUpdate]);

  if (phase === "idle") return null;

  const tone =
    phase === "error"
      ? "border-red-200 bg-red-50"
      : phase === "current"
        ? "border-line bg-surface"
        : "border-tomato-100 bg-tomato-50";

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
                : "更新失败";

  const hint =
    phase === "checking"
      ? "正在从 GitHub Release 获取版本信息。"
      : phase === "available"
        ? `当前版本 ${currentVersion}，下载完成后由你决定何时重启。`
        : phase === "downloading"
          ? "下载期间请保持应用运行，完成后会提示你重启。"
          : phase === "ready"
            ? "点击重启并安装，安装程序会在完成后自动重新打开 TomatoDesk。"
            : phase === "installing"
              ? "应用即将退出，安装完成后会自动重新打开。"
              : phase === "current"
                ? "当前已是最新发布版本。"
                : (message ?? "更新过程出现未知错误。");

  const primaryClass =
    "rounded-md bg-tomato-600 px-3 py-2 text-sm font-semibold text-white transition hover:bg-tomato-500 disabled:opacity-50";
  const secondaryClass =
    "rounded-md border border-line bg-surface px-3 py-2 text-sm font-medium text-ink-700 hover:bg-canvas disabled:opacity-50";

  return (
    <section className={`border-b px-5 py-3 sm:px-7 ${tone}`} aria-live="polite" role="status">
      <div className="flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
        <div className="min-w-0">
          <p
            className={`text-sm font-semibold ${phase === "error" ? "text-red-700" : phase === "current" ? "text-ink-950" : "text-tomato-600"}`}
          >
            {title}
          </p>
          <p className="mt-1 text-xs leading-5 text-ink-700">{hint}</p>

          {notes && (phase === "available" || phase === "ready") ? (
            <details className="mt-2 max-w-2xl">
              <summary className="cursor-pointer text-xs font-semibold text-ink-700">
                更新说明
              </summary>
              <pre className="mt-2 max-h-40 overflow-auto whitespace-pre-wrap rounded-md border border-line bg-surface p-3 text-xs leading-5 text-ink-700">
                {notes}
              </pre>
            </details>
          ) : null}

          {phase === "downloading" ? (
            <div className="mt-2 max-w-md">
              <div className="h-1.5 w-full overflow-hidden rounded-full bg-tomato-100">
                <div
                  className="h-full rounded-full bg-tomato-600 transition-[width] duration-300"
                  style={{ width: `${percent}%` }}
                />
              </div>
              <p className="mt-1 text-[11px] text-ink-500">
                {total > 0
                  ? `${formatBytes(received)} / ${formatBytes(total)} · ${percent}%`
                  : `${formatBytes(received)} 已下载`}
              </p>
            </div>
          ) : null}
        </div>

        <div className="flex shrink-0 flex-wrap items-center gap-2">
          {phase === "checking" ? <span className="text-xs text-ink-500">请稍候…</span> : null}
          {phase === "available" ? (
            <>
              <button className={primaryClass} type="button" onClick={() => void startDownload()}>
                下载并安装
              </button>
              <button className={secondaryClass} type="button" onClick={skipVersion}>
                跳过此版本
              </button>
              <button className={secondaryClass} type="button" onClick={dismiss}>
                稍后
              </button>
            </>
          ) : null}
          {phase === "downloading" ? (
            <button className={secondaryClass} type="button" disabled>
              下载中…
            </button>
          ) : null}
          {phase === "ready" ? (
            <>
              <button className={primaryClass} type="button" onClick={() => void installNow()}>
                重启并安装
              </button>
              <button className={secondaryClass} type="button" onClick={dismiss}>
                稍后
              </button>
            </>
          ) : null}
          {phase === "installing" ? <span className="text-xs text-ink-500">请稍候…</span> : null}
          {phase === "current" ? (
            <button className={secondaryClass} type="button" onClick={dismiss}>
              关闭
            </button>
          ) : null}
          {phase === "error" ? (
            <>
              <button className={primaryClass} type="button" onClick={() => void runCheck(true)}>
                重试
              </button>
              <button className={secondaryClass} type="button" onClick={dismiss}>
                关闭
              </button>
            </>
          ) : null}
        </div>
      </div>
    </section>
  );
}

export default UpdateBanner;
