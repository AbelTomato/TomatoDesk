import { check, type Update } from "@tauri-apps/plugin-updater";

const SKIPPED_VERSION_KEY = "tomatodesk:skipped-update-version";

/** 检查 GitHub Release 上的更新清单，超时视为无更新。 */
export async function findUpdate(): Promise<Update | null> {
  return check({ timeout: 15000 });
}

/** 读取用户主动跳过的版本号。 */
export function readSkippedVersion(): string | null {
  try {
    return window.localStorage.getItem(SKIPPED_VERSION_KEY);
  } catch {
    return null;
  }
}

/** 记录用户跳过的版本号，之后自动检查不再提示该版本。 */
export function rememberSkippedVersion(version: string) {
  try {
    window.localStorage.setItem(SKIPPED_VERSION_KEY, version);
  } catch {
    // localStorage 不可用时只是重复提示，不影响更新流程。
  }
}

/** 用户重新手动检查时清除跳过记录。 */
export function clearSkippedVersion() {
  try {
    window.localStorage.removeItem(SKIPPED_VERSION_KEY);
  } catch {
    // 同上。
  }
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / 1024 ** exponent;
  return `${value >= 10 || exponent === 0 ? Math.round(value) : value.toFixed(1)} ${units[exponent]}`;
}

export function describeUpdateError(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error ?? "");
  if (/network|connect|timeout|dns|request/i.test(message)) {
    return "网络不可用或访问 GitHub 失败，稍后会自动重试。";
  }
  if (/signature|verify|invalid/i.test(message)) {
    return "更新包校验失败，已中止安装。请稍后重试或手动下载安装包。";
  }
  return message || "更新过程出现未知错误。";
}
