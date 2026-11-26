import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

import AppShell from "./app/AppShell";
import QuickCapture from "./features/quick-capture/QuickCapture";
import StyleGuide from "./features/dev/StyleGuide";

const params = new URLSearchParams(window.location.search);
const showStyleGuide = import.meta.env.DEV && params.has("styleguide");
/** 开发模式下单独预览快速收集窗口的排版，不需要启动 Tauri 窗口。 */
const showCapturePreview = import.meta.env.DEV && params.has("capture");

function App() {
  if (showStyleGuide) {
    return <StyleGuide />;
  }

  if (showCapturePreview) {
    return <QuickCapture />;
  }

  if (isTauri() && getCurrentWindow().label === "quick-capture") {
    return <QuickCapture />;
  }

  return <AppShell />;
}

export default App;
