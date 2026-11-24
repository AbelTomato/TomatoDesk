import { useState } from "react";

import { appEnvironment } from "./lib/env";
import { getAppInfo, type AppInfo } from "./lib/tauri";

const layers = [
  { name: "Capture", path: "src/features/capture" },
  { name: "Inbox", path: "src/features/inbox" },
  { name: "GitHub", path: "src/features/github" },
  { name: "Search", path: "src/features/search" },
];

function App() {
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function checkBackend() {
    setError(null);
    try {
      setAppInfo(await getAppInfo());
    } catch {
      setError("无法连接 Rust Command，请在 Tauri 窗口中重试。");
    }
  }

  return (
    <main className="min-h-screen bg-canvas px-6 py-10 text-ink-950 sm:px-10">
      <div className="mx-auto flex max-w-5xl flex-col gap-10">
        <header className="flex flex-col gap-4 border-b border-line pb-8 sm:flex-row sm:items-end sm:justify-between">
          <div>
            <p className="mb-2 text-sm font-semibold uppercase tracking-[0.18em] text-tomato-600">
              TomatoDesk
            </p>
            <h1 className="text-3xl font-semibold tracking-tight sm:text-4xl">
              Personal inbox, ready for the next step.
            </h1>
            <p className="mt-3 max-w-2xl text-base leading-7 text-ink-700">
              工程骨架已就绪。后续步骤会在这个本地优先的工作台中加入收集、搜索和同步能力。
            </p>
          </div>
          <span className="w-fit rounded-full border border-line bg-surface px-3 py-1 text-sm text-ink-500">
            {appEnvironment}
          </span>
        </header>

        <section className="grid gap-5 md:grid-cols-[minmax(0,1fr)_minmax(15rem,0.65fr)]">
          <div className="rounded-lg border border-line bg-surface p-6 shadow-sm">
            <div className="flex items-start justify-between gap-4">
              <div>
                <p className="text-sm font-medium text-ink-500">Step A / 工程骨架</p>
                <h2 className="mt-2 text-xl font-semibold">前后端 Command 已连接</h2>
              </div>
              <div className="rounded-full bg-tomato-100 px-2.5 py-1 text-xs font-semibold text-tomato-600">
                ready
              </div>
            </div>
            <p className="mt-5 max-w-xl text-sm leading-6 text-ink-700">
              点击按钮调用 Rust 侧的类型化 Command，确认 Tauri IPC 通道已经可以完成一次请求。
            </p>
            <button
              className="mt-6 rounded-md bg-tomato-600 px-4 py-2.5 text-sm font-semibold text-white transition hover:bg-tomato-500 focus:outline-2 focus:outline-offset-2 focus:outline-tomato-600"
              type="button"
              onClick={checkBackend}
            >
              检查 Rust Command
            </button>
            {appInfo ? (
              <p className="mt-4 rounded-md bg-tomato-50 px-3 py-2 text-sm text-tomato-600">
                {appInfo.name} {appInfo.version} · {appInfo.environment}
              </p>
            ) : null}
            {error ? <p className="mt-4 text-sm text-red-700">{error}</p> : null}
          </div>

          <aside className="rounded-lg border border-line bg-ink-950 p-6 text-white shadow-sm">
            <p className="text-sm font-medium text-white/60">目录边界</p>
            <ul className="mt-5 space-y-4">
              {layers.map((layer) => (
                <li key={layer.name} className="flex items-center justify-between gap-4 text-sm">
                  <span className="font-medium">{layer.name}</span>
                  <code className="text-right text-xs text-white/55">{layer.path}</code>
                </li>
              ))}
            </ul>
          </aside>
        </section>
      </div>
    </main>
  );
}

export default App;
