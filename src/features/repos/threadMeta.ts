import type { StateTone } from "../../components/ui";
import type { ThreadState, ThreadSummary } from "../../lib/api";

/**
 * GitHub 原生概念一律沿用其英文小写文案，与网页端保持一致；
 * 自有概念（概览、我的条目等）才用中文。见 docs/design/design-language.md 第七节。
 */
export const stateLabels: Record<ThreadState, string> = {
  open: "open",
  draft: "draft",
  merged: "merged",
  closed: "closed",
};

/** 状态色例外只作用于这里：draft 视为中性，其余三态各占一色。 */
export const stateTones: Record<ThreadState, StateTone> = {
  open: "open",
  draft: "neutral",
  merged: "merged",
  closed: "closed",
};

export const reviewScopeLabels: Record<"review_requested" | "reviewed", string> = {
  review_requested: "review requested",
  reviewed: "reviewed",
};

/** 一条线程与「我」的关系，优先级从强到弱；无关系返回 null。 */
export function relationLabel(thread: ThreadSummary): string | null {
  if (thread.isAuthor) return "author";
  if (thread.reviewRequested) return "review requested";
  if (thread.reviewed) return "reviewed";
  if (thread.isAssigned) return "assigned";
  return null;
}

/** 列表行与详情页共用：`PR #476` / `Issue #12`。 */
export function threadHeadline(thread: ThreadSummary): string {
  return `${thread.kind === "pull_request" ? "PR" : "Issue"} #${thread.number}`;
}
