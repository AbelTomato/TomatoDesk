import type { InboxSourceKind, TriageOutcome } from "../../lib/api";

const categoryLabels: Record<string, string> = {
  mention: "提及",
  review_requested: "评审请求",
  assigned: "指派",
  ci_failed: "CI 失败",
  release: "发布",
  mail: "邮件",
  comment: "评论",
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
