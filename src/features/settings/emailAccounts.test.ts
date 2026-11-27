import { describe, expect, it } from "vitest";

import type { EmailAccountStatus } from "../../lib/api";
import {
  MAX_EMAIL_ACCOUNTS,
  canAddAccount,
  defaultPort,
  describeServer,
  suggestServer,
} from "./emailAccounts";

const account = (overrides: Partial<EmailAccountStatus> = {}): EmailAccountStatus => ({
  id: "source-email-test",
  address: "me@qq.com",
  host: "imap.qq.com",
  port: 993,
  tls: "implicit",
  mailbox: "INBOX",
  enabled: true,
  lastSyncedAt: null,
  lastError: null,
  ...overrides,
});

describe("suggestServer", () => {
  it("prefills the hosts documented for common providers", () => {
    expect(suggestServer("me@qq.com")).toEqual({ host: "imap.qq.com", port: 993 });
    expect(suggestServer("me@163.com")).toEqual({ host: "imap.163.com", port: 993 });
    expect(suggestServer("me@gmail.com")).toEqual({ host: "imap.gmail.com", port: 993 });
    expect(suggestServer("me@icloud.com")).toEqual({ host: "imap.mail.me.com", port: 993 });
  });

  it("normalizes case and surrounding spaces", () => {
    expect(suggestServer(" Me@QQ.com ")).toEqual({ host: "imap.qq.com", port: 993 });
    expect(suggestServer("me@Gmail.COM")).toEqual({ host: "imap.gmail.com", port: 993 });
  });

  it("returns null when there is no domain to look up", () => {
    expect(suggestServer("me@example.com")).toBeNull();
    expect(suggestServer("me")).toBeNull();
    expect(suggestServer("@qq.com")).toBeNull();
    expect(suggestServer("")).toBeNull();
  });
});

describe("defaultPort", () => {
  it("follows the tls mode", () => {
    expect(defaultPort("implicit")).toBe(993);
    expect(defaultPort("starttls")).toBe(143);
  });
});

describe("canAddAccount", () => {
  it("allows additions up to the backend limit", () => {
    const accounts = Array.from({ length: MAX_EMAIL_ACCOUNTS - 1 }, (_, index) =>
      account({ id: `source-email-${index}` }),
    );
    expect(canAddAccount(accounts)).toBe(true);
    expect(canAddAccount([...accounts, account({ id: "source-email-last" })])).toBe(false);
  });

  it("does not count soft-disabled accounts", () => {
    const accounts = Array.from({ length: MAX_EMAIL_ACCOUNTS }, (_, index) =>
      account({ id: `source-email-${index}` }),
    );
    accounts[0] = { ...accounts[0], enabled: false };
    expect(canAddAccount(accounts)).toBe(true);
  });
});

describe("describeServer", () => {
  it("keeps the port visible so same-name hosts can be told apart", () => {
    expect(describeServer(account())).toBe("imap.qq.com:993");
    expect(describeServer(account({ host: "imap.163.com", port: 143 }))).toBe("imap.163.com:143");
  });
});
