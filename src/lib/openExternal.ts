import { isTauri } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";

/** 桌面端交给系统浏览器，浏览器预览时退回 `window.open`。 */
export async function openExternal(url: string) {
  if (isTauri()) await openUrl(url);
  else window.open(url, "_blank", "noopener,noreferrer");
}
