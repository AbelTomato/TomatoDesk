import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

import QuickCapture from "./features/capture/QuickCapture";
import Inbox from "./features/inbox/Inbox";

function App() {
  if (isTauri() && getCurrentWindow().label === "quick-capture") {
    return <QuickCapture />;
  }

  return <MainApp />;
}

function MainApp() {
  return <Inbox />;
}

export default App;
