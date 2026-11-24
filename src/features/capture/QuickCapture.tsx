import { type FormEvent, useEffect, useRef, useState } from "react";
import { readText } from "@tauri-apps/plugin-clipboard-manager";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { captureItem, dismissCapture } from "../../lib/api";

const MAX_CAPTURE_LENGTH = 16_384;

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
  const [error, setError] = useState<string | null>(null);
  const valueRef = useRef<HTMLTextAreaElement | null>(null);
  const urlRef = useRef<HTMLInputElement | null>(null);
  const linkUrl = normalizeWebUrl(value);
  const overLimit = value.length > MAX_CAPTURE_LENGTH || note.length > MAX_CAPTURE_LENGTH;

  useEffect(() => {
    const currentWindow = getCurrentWindow();
    let disposed = false;
    let unlisten: (() => void) | undefined;

    async function loadClipboard() {
      setValue("");
      setNote("");
      setError(null);
      setSaving(false);
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
      unlisten?.();
      if (!unlisten) void listener.then((stopListening) => stopListening()).catch(() => {});
    };
  }, []);

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!value.trim() || overLimit || saving) return;

    setSaving(true);
    setError(null);
    try {
      await captureItem({ value, note: linkUrl ? note : null });
    } catch (captureError) {
      setError(captureError instanceof Error ? captureError.message : "保存失败，请重试。");
      setSaving(false);
    }
  }

  async function close() {
    try {
      await dismissCapture();
    } catch {
      setError("窗口关闭失败，请重试。");
    }
  }

  return (
    <main className="min-h-screen bg-canvas px-5 py-4 text-ink-950">
      <form className="mx-auto flex max-w-xl flex-col gap-3" onSubmit={submit}>
        <header className="flex items-center justify-between">
          <div>
            <p className="text-xs font-semibold uppercase text-tomato-600">TomatoDesk</p>
            <h1 className="mt-1 text-lg font-semibold">快速收集</h1>
          </div>
          <button
            className="rounded-md border border-line px-3 py-1.5 text-sm text-ink-700 hover:bg-surface focus:outline-2 focus:outline-offset-2 focus:outline-tomato-600"
            type="button"
            onClick={() => void close()}
          >
            关闭
          </button>
        </header>

        {linkUrl ? (
          <>
            <label className="flex flex-col gap-1 text-sm font-medium text-ink-700">
              链接
              <input
                ref={urlRef}
                className="rounded-md border border-line bg-surface px-3 py-2 text-sm text-ink-950 outline-none focus:border-tomato-600 focus:ring-2 focus:ring-tomato-100"
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
            <label className="flex flex-col gap-1 text-sm font-medium text-ink-700">
              备注
              <textarea
                className="min-h-20 resize-y rounded-md border border-line bg-surface px-3 py-2 text-sm text-ink-950 outline-none focus:border-tomato-600 focus:ring-2 focus:ring-tomato-100"
                value={note}
                maxLength={MAX_CAPTURE_LENGTH}
                placeholder="添加备注（可选）"
                onChange={(event) => {
                  setNote(event.target.value);
                  setError(null);
                }}
              />
            </label>
            <p className="text-xs leading-5 text-ink-500">
              保存时会尝试读取标题、描述和规范链接；网络失败时仍保留 URL 与备注。
            </p>
          </>
        ) : (
          <label className="flex flex-col gap-1 text-sm font-medium text-ink-700">
            笔记内容
            <textarea
              ref={valueRef}
              className="min-h-40 resize-y rounded-md border border-line bg-surface px-3 py-2 text-sm text-ink-950 outline-none focus:border-tomato-600 focus:ring-2 focus:ring-tomato-100"
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

        <footer className="flex items-center justify-between gap-3 border-t border-line pt-3">
          <p className="min-w-0 text-xs text-red-700" role="status">
            {error ?? (overLimit ? `内容不能超过 ${MAX_CAPTURE_LENGTH} 个字符。` : "")}
          </p>
          <button
            className="shrink-0 rounded-md bg-tomato-600 px-4 py-2 text-sm font-semibold text-white transition hover:bg-tomato-500 focus:outline-2 focus:outline-offset-2 focus:outline-tomato-600 disabled:cursor-not-allowed disabled:opacity-50"
            type="submit"
            disabled={!value.trim() || overLimit || saving}
          >
            {saving ? "保存中…" : "保存到 Inbox"}
          </button>
        </footer>
      </form>
    </main>
  );
}

export default QuickCapture;
