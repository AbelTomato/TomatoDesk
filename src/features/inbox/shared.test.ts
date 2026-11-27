import { describe, expect, it } from "vitest";

import type { InboxSource } from "../../lib/api";
import { emptyHints, inboxTabOrder, isEmailMessage, latestSyncedAt, tabLabels } from "./shared";

const source = (overrides: Partial<InboxSource> = {}): InboxSource => ({
  id: "source-github",
  kind: "github",
  name: "GitHub 通知",
  enabled: true,
  lastSyncedAt: null,
  lastError: null,
  createdAt: "2026-11-20T00:00:00.000Z",
  updatedAt: "2026-11-20T00:00:00.000Z",
  ...overrides,
});

describe("latestSyncedAt", () => {
  it("picks the newest sync across every enabled source", () => {
    const sources = [
      source({ id: "source-github", lastSyncedAt: "2026-11-27T01:50:00.000Z" }),
      source({
        id: "source-email-qq",
        kind: "email",
        lastSyncedAt: "2026-11-27T02:00:00.000Z",
      }),
      source({
        id: "source-email-163",
        kind: "email",
        lastSyncedAt: "2026-11-26T20:00:00.000Z",
      }),
    ];
    expect(latestSyncedAt(sources)).toBe("2026-11-27T02:00:00.000Z");
  });

  it("ignores disabled and never-synced sources", () => {
    const sources = [
      source({ id: "source-github", lastSyncedAt: "2026-11-27T01:50:00.000Z" }),
      source({
        id: "source-email-old",
        kind: "email",
        enabled: false,
        lastSyncedAt: "2026-11-27T03:00:00.000Z",
      }),
      source({ id: "source-manual", kind: "manual" }),
    ];
    expect(latestSyncedAt(sources)).toBe("2026-11-27T01:50:00.000Z");
  });

  it("skips values that cannot be parsed and returns null when nothing is left", () => {
    expect(latestSyncedAt([source({ lastSyncedAt: "not-a-date" })])).toBeNull();
    expect(latestSyncedAt([])).toBeNull();
  });
});

describe("inbox tabs", () => {
  it("keeps four tabs in status order with a label and an empty state each", () => {
    expect(inboxTabOrder).toEqual(["pending", "task", "dismissed", "all"]);
    for (const tab of inboxTabOrder) {
      expect(tabLabels[tab]).toBeTruthy();
      expect(emptyHints[tab].title).toBeTruthy();
      expect(emptyHints[tab].hint).toBeTruthy();
    }
  });

  it("names each outcome instead of one generic triaged bucket", () => {
    expect(tabLabels).toEqual({
      pending: "待处理",
      task: "已转待办",
      dismissed: "已忽略",
      all: "全部",
    });
  });
});

describe("isEmailMessage", () => {
  it("treats email sources and mail categories as email", () => {
    expect(isEmailMessage(source({ kind: "email" }), "mention")).toBe(true);
    expect(isEmailMessage(null, "mail")).toBe(true);
  });

  it("keeps GitHub notifications on the GitHub branch", () => {
    expect(isEmailMessage(source({ kind: "github" }), "mention")).toBe(false);
    expect(isEmailMessage(source({ kind: "manual" }), "notification")).toBe(false);
  });
});
