import { type FormEvent, useEffect, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { readText } from "@tauri-apps/plugin-clipboard-manager";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { Button, Input, Textarea } from "../../components/ui";
import { captureFromClipboard, dismissCapture, finishCapture } from "../../lib/api";

const MAX_CAPTURE_LENGTH = 16_384;
/** 成功反馈停留一小会儿再收起窗口，足够看清但不打断手头的事。 */
const SAVED_HINT_MS = 1000;

function normalizeWebUrl(value: string): string | null {
  const trimmed = value.trim();
  if (!trimmed || /\s/.test(trimmed)) {
    return null;
  }
  if (!/^https?:\/\//i.test(trimmed) && trimmed.includes(":")) {
    return null;
  }

  try {
    const candidate = /^https?:\/\//i.test(trimmed) ? trimmed : `https://${trimmed}`;
    const url = new URL(candidate);
    const host = url.hostname.replace(/^\[|\]$/g, "");
    if (
      (url.protocol === "http:" || url.protocol === "https:") &&
      (host.includes(".") || host === "localhost" || host.includes(":"))
    ) {
      return url.toString();
    }
  } catch {
    return null;
  }
  return null;
}

function QuickCapture() {
  const [value, setValue] = useState("");
  const [note, setNote] = useState("");
  const [readingClipboard, setReadingClipboard] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const valueRef = useRef<HTMLTextAreaElement | null>(null);
  const urlRef = useRef<HTMLInputElement | null>(null);
  const hideTimerRef = useRef<number | null>(null);
  const linkUrl = normalizeWebUrl(value);
  const overLimit = value.length > MAX_CAPTURE_LENGTH || note.length > MAX_CAPTURE_LENGTH;

  useEffect(() => {
    // 浏览器里预览排版时没有 Tauri 窗口与剪贴板插件，跳过读取。
    if (!isTauri()) return;
    const currentWindow = getCurrentWindow();
    let disposed = false;
    let unlisten: (() => void) | undefined;

    async function loadClipboard() {
      if (hideTimerRef.current !== null) {
        window.clearTimeout(hideTimerRef.current);
        hideTimerRef.current = null;
      }
      setValue("");
      setNote("");
      setError(null);
      setSaving(false);
      setSaved(false);
      setReadingClipboard(true);
      try {
        const text = (await readText()).trim();
        if (disposed) return;
        setValue(normalizeWebUrl(text) ?? text);
        requestAnimationFrame(() => {
          if (normalizeWebUrl(text)) urlRef.current?.focus();
          else valueRef.current?.focus();
        });
      } catch {
        if (!disposed) setError("读取剪贴板失败，可直接输入内容。");
      } finally {
        if (!disposed) setReadingClipboard(false);
      }
    }

    const listener = currentWindow.listen("quick-capture-opened", () => {
      void loadClipboard();
    });
    void listener
      .then((stopListening) => {
        if (disposed) stopListening();
        else unlisten = stopListening;
      })
      .catch(() => {});
    void currentWindow
      .isVisible()
      .then((visible) => {
        if (visible && !disposed) void loadClipboard();
      })
      .catch(() => {});

    return () => {
      disposed = true;
      if (hideTimerRef.current !== null) {
        window.clearTimeout(hideTimerRef.current);
        hideTimerRef.current = null;
      }
      unlisten?.();
      if (!unlisten) void listener.then((stopListening) => stopListening()).catch(() => {});
    };
  }, []);

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!value.trim() || overLimit || saving || saved) return;

    setSaving(true);
    setError(null);
    try {
      await captureFromClipboard({ value, note: linkUrl ? note : null });
      setSaving(false);
      setSaved(true);
      // 先留在原页面给一句成功提示，再收起；不打开主窗口。
      hideTimerRef.current = window.setTimeout(() => {
        hideTimerRef.current = null;
        void finishCapture().catch(() => undefined);
      }, SAVED_HINT_MS);
    } catch (captureError) {
      setError(captureError instanceof Error ? captureError.message : "保存失败，请重试。");
      setSaving(false);
    }
  }

  async function close() {
    if (hideTimerRef.current !== null) {
      window.clearTimeout(hideTimerRef.current);
      hideTimerRef.current = null;
    }
    try {
      await dismissCapture();
    } catch {
      setError("窗口关闭失败，请重试。");
    }
  }

  return (
    <main className="min-h-screen bg-background px-4 py-3 text-foreground">
      <form className="mx-auto flex max-w-xl flex-col gap-3" onSubmit={submit}>
        <header className="flex items-center justify-between gap-3 border-b border-border pb-2">
          <div className="flex items-baseline gap-2">
            <h1 className="text-base font-bold tracking-tight">随手记一条</h1>
            <span className="font-mono text-xs text-muted-foreground">
              {linkUrl ? "链接" : "笔记"}
            </span>
          </div>
          <Button variant="ghost" size="sm" onClick={() => void close()}>
            关闭
          </Button>
        </header>

        {linkUrl ? (
          <>
            <label className="flex flex-col gap-1">
              <span className="text-xs font-medium text-muted-foreground">链接</span>
              <Input
                ref={urlRef}
                type="text"
                inputMode="url"
                value={value}
                maxLength={MAX_CAPTURE_LENGTH}
                onChange={(event) => {
                  setValue(event.target.value);
                  setError(null);
                }}
              />
            </label>
            <label className="flex flex-col gap-1">
              <span className="text-xs font-medium text-muted-foreground">备注</span>
              <Textarea
                value={note}
                maxLength={MAX_CAPTURE_LENGTH}
                placeholder="添加备注（可选）"
                onChange={(event) => {
                  setNote(event.target.value);
                  setError(null);
                }}
              />
            </label>
            <p className="text-xs leading-5 text-muted-foreground">
              保存后先用站点和路径命名，网页标题会在后台自动补全。
            </p>
          </>
        ) : (
          <label className="flex flex-col gap-1">
            <span className="text-xs font-medium text-muted-foreground">内容</span>
            <Textarea
              ref={valueRef}
              rows={8}
              value={value}
              maxLength={MAX_CAPTURE_LENGTH}
              placeholder={readingClipboard ? "正在读取剪贴板…" : "粘贴或输入文本"}
              onChange={(event) => {
                setValue(event.target.value);
                setError(null);
              }}
            />
          </label>
        )}

        <footer className="flex items-center justify-between gap-3 border-t border-border pt-3">
          <p
            className={`min-w-0 text-xs ${saved ? "text-success" : "text-destructive"}`}
            role="status"
          >
            {saved
              ? "已保存到随手记"
              : (error ?? (overLimit ? `内容不能超过 ${MAX_CAPTURE_LENGTH} 个字符。` : ""))}
          </p>
          <Button
            type="submit"
            className="shrink-0"
            disabled={!value.trim() || overLimit || saving || saved}
          >
            {saved ? "已保存" : saving ? "保存中…" : "保存到随手记"}
          </Button>
        </footer>
      </form>
    </main>
  );
}

export default QuickCapture;
