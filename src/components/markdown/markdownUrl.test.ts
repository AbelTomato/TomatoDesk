import { describe, expect, it } from "vitest";

import { resolveImageUrl, resolveLinkUrl } from "./markdownUrl";

const base = "https://github.com/TencentCloud/Octop/pull/482";

describe("resolveLinkUrl", () => {
  it("keeps http / https / mailto links", () => {
    expect(resolveLinkUrl("https://example.com/a?b=1", base)).toBe("https://example.com/a?b=1");
    expect(resolveLinkUrl("http://example.com", base)).toBe("http://example.com/");
    expect(resolveLinkUrl("mailto:someone@example.com", base)).toBe("mailto:someone@example.com");
  });

  it("resolves relative paths and anchors against the thread url", () => {
    expect(resolveLinkUrl("../files", base)).toBe("https://github.com/TencentCloud/Octop/files");
    expect(resolveLinkUrl("#issuecomment-123", base)).toBe(`${base}#issuecomment-123`);
  });

  it("rejects dangerous protocols, including obfuscated ones", () => {
    for (const value of [
      "javascript:alert(1)",
      "JaVaScRiPt:alert(1)",
      "java\nscript:alert(1)",
      "data:text/html;base64,PHNjcmlwdD4=",
      "vbscript:msgbox(1)",
      "file:///C:/Windows/System32/calc.exe",
      "//evil.example/path",
      "",
      "   ",
      null,
      undefined,
    ]) {
      expect(resolveLinkUrl(value, base)).toBeNull();
    }
  });

  it("returns null when the base url is unusable", () => {
    expect(resolveLinkUrl("/relative", "not-a-url")).toBeNull();
  });
});

describe("resolveImageUrl", () => {
  it("only allows https images", () => {
    expect(resolveImageUrl("https://avatars.githubusercontent.com/u/1", base)).toBe(
      "https://avatars.githubusercontent.com/u/1",
    );
    expect(resolveImageUrl("http://example.com/a.png", base)).toBeNull();
    expect(resolveImageUrl("data:image/png;base64,AAAA", base)).toBeNull();
    expect(resolveImageUrl("javascript:alert(1)", base)).toBeNull();
  });

  it("resolves relative image paths but still requires https", () => {
    expect(resolveImageUrl("./assets/cover.png", base)).toBe(
      "https://github.com/TencentCloud/Octop/pull/assets/cover.png",
    );
  });
});
