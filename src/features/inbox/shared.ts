import type { InboxSource, InboxSourceKind, InboxTab, TriageOutcome } from "../../lib/api";

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
  task: "已转待办",
  capture: "已转为随手记",
};

/** Tab 顺序即状态轴顺序；「全部」放最后，计数恰好是前三个之和。 */
export const inboxTabOrder: InboxTab[] = ["pending", "task", "dismissed", "all"];

export const tabLabels: Record<InboxTab, string> = {
  pending: "待处理",
  task: "已转待办",
  dismissed: "已忽略",
  all: "全部",
};

/** 空态只说现在的状态，不解释模型；「只看未读」另有自己的空态。 */
export const emptyHints: Record<InboxTab, { title: string; hint: string }> = {
  pending: {
    title: "收件箱已清空",
    hint: "新消息会在下次同步后出现在这里。",
  },
  task: {
    title: "还没有转成待办的消息",
    hint: "把需要跟进的消息转为待办后，会留在这里备查。",
  },
  dismissed: {
    title: "还没有忽略的消息",
    hint: "忽略的消息会留在这里，需要时可以回头找。",
  },
  all: {
    title: "收件箱是空的",
    hint: "配置 GitHub Token 或邮箱账户后，消息会自动同步进来。",
  },
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
