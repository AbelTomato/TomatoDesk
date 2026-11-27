import type { InboxSource, InboxSourceKind, TriageOutcome } from "../../lib/api";

const categoryLabels: Record<string, string> = {
  mention: "提及",
  review_requested: "评审请求",
  assigned: "指派",
  ci_failed: "CI 失败",
  release: "发布",
  mail: "邮件",
  comment: "评论",
  notification: "其他通知",
};

export function describeCategory(category: string): string {
  return categoryLabels[category] ?? category;
}

/** 分类下拉只列后端会写入的几类，其余值仍按原文显示。 */
export const categoryOptions: Array<{ value: string; label: string }> = [
  { value: "mention", label: "提及" },
  { value: "review_requested", label: "评审请求" },
  { value: "assigned", label: "指派" },
  { value: "ci_failed", label: "CI 失败" },
  { value: "release", label: "发布" },
  { value: "comment", label: "评论" },
  { value: "notification", label: "其他通知" },
  { value: "mail", label: "邮件" },
];

export const sourceKindLabels: Record<InboxSourceKind, string> = {
  github: "GitHub",
  email: "邮箱",
  rss: "RSS",
  manual: "手动",
};

export const triageLabels: Record<TriageOutcome, string> = {
  dismissed: "已忽略",
  task: "已转为待办",
  capture: "已提取为随手记",
};

/**
 * 头部「自动同步」取**已启用来源里最新的一次**：多账户时任一账户同步完都算刷新，
 * 只盯 GitHub 会让邮件账户的同步时间永远显示不出来。
 */
export function latestSyncedAt(sources: InboxSource[]): string | null {
  let latest: { value: string; time: number } | null = null;
  for (const source of sources) {
    if (!source.enabled || !source.lastSyncedAt) continue;
    const time = Date.parse(source.lastSyncedAt);
    if (Number.isNaN(time)) continue;
    if (!latest || time > latest.time) latest = { value: source.lastSyncedAt, time };
  }
  return latest?.value ?? null;
}

/** 邮件来源的详情页不出现 GitHub 动作；来源缺失时按分类兜底。 */
export function isEmailMessage(source: InboxSource | null, category: string): boolean {
  return source?.kind === "email" || category === "mail";
}
