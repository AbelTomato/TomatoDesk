/**
 * Markdown 里的链接与图片地址校验。
 *
 * 顺序很重要：先按线程地址解析成绝对地址，再校验协议。
 * 反过来的话 `javascript:alert(1)` 这类值可能被 base 掩盖成相对路径。
 */

const LINK_PROTOCOLS = new Set(["http:", "https:", "mailto:"]);
const IMAGE_PROTOCOLS = new Set(["https:"]);

function parseUrl(raw: string | null | undefined, baseUrl: string): URL | null {
  const value = raw?.trim();
  if (!value) return null;
  // 协议相对地址（//evil.example）没有明确来源，一律拒绝。
  if (value.startsWith("//")) return null;
  try {
    return new URL(value, baseUrl);
  } catch {
    return null;
  }
}

/** 链接：仅 http / https / mailto；相对路径与锚点按 `baseUrl` 解析。 */
export function resolveLinkUrl(raw: string | null | undefined, baseUrl: string): string | null {
  const url = parseUrl(raw, baseUrl);
  if (!url || !LINK_PROTOCOLS.has(url.protocol)) return null;
  return url.toString();
}

/** 图片：仅 https；GitHub 的图片基本都是绝对地址。 */
export function resolveImageUrl(raw: string | null | undefined, baseUrl: string): string | null {
  const url = parseUrl(raw, baseUrl);
  if (!url || !IMAGE_PROTOCOLS.has(url.protocol)) return null;
  return url.toString();
}
