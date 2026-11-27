import type { EmailAccountStatus, EmailTls } from "../../lib/api";

/** 与后端 `MAX_EMAIL_ACCOUNTS` 保持一致，超出时后端会返回 validation。 */
export const MAX_EMAIL_ACCOUNTS = 5;

/**
 * 服务商预填只给主机与默认端口，不猜地址，也不做域名特判之外的逻辑：
 * 协议层靠能力探测兼容，这里纯粹是省一次手输。
 */
const presets: Array<{ domain: string; host: string; port: number }> = [
  { domain: "qq.com", host: "imap.qq.com", port: 993 },
  { domain: "163.com", host: "imap.163.com", port: 993 },
  { domain: "gmail.com", host: "imap.gmail.com", port: 993 },
  { domain: "icloud.com", host: "imap.mail.me.com", port: 993 },
];

/** 用户没填端口时按加密方式给默认值，与后端 `EmailTls::default_port` 一致。 */
export function defaultPort(tls: EmailTls): number {
  return tls === "starttls" ? 143 : 993;
}

/** 按邮箱域名给默认服务器；认不出来时返回 null，交给用户自己填。 */
export function suggestServer(address: string): { host: string; port: number } | null {
  const at = address.lastIndexOf("@");
  // 还没输入名字时不预填，避免用户刚敲一个 @ 主机就被改掉。
  if (at <= 0) return null;
  const domain = address
    .slice(at + 1)
    .trim()
    .toLowerCase();
  const preset = presets.find((entry) => entry.domain === domain);
  return preset ? { host: preset.host, port: preset.port } : null;
}

/** 上限只数启用中的账户；已软停用的账户在列表里消失，也不占名额。 */
export function canAddAccount(accounts: EmailAccountStatus[]): boolean {
  return accounts.filter((account) => account.enabled).length < MAX_EMAIL_ACCOUNTS;
}

/** 主机加端口的展示形式，设置页用它区分同名不同服务器的账户。 */
export function describeServer(account: EmailAccountStatus): string {
  return `${account.host}:${account.port}`;
}
