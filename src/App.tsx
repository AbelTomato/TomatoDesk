import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

import AppShell from "./app/AppShell";
import QuickCapture from "./features/capture/QuickCapture";
import StyleGuide from "./features/dev/StyleGuide";

const showStyleGuide =
  import.meta.env.DEV && new URLSearchParams(window.location.search).has("styleguide");

function App() {
  if (showStyleGuide) {
    return <StyleGuide />;
  }

  if (isTauri() && getCurrentWindow().label === "quick-capture") {
    return <QuickCapture />;
  }

  return <AppShell />;
}

export default App;
