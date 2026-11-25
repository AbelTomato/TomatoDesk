import type { ReactNode } from "react";

import { cn } from "./cn";

export type StateTone = "open" | "merged" | "closed" | "neutral";

const tones: Record<StateTone, { bar: string; text: string }> = {
  open: { bar: "bg-state-open", text: "text-state-open" },
  merged: { bar: "bg-state-merged", text: "text-state-merged" },
  closed: { bar: "bg-state-closed", text: "text-state-closed" },
  neutral: { bar: "bg-muted-foreground/60", text: "text-muted-foreground" },
};

interface StateMarkProps {
  tone?: StateTone;
  children: ReactNode;
  className?: string;
}

/**
 * 状态标记：2px 竖条 + 小号等宽文字。
 * 状态色只出现在这里，不做填充徽标，避免破坏中性基调。
 */
export function StateMark({ tone = "neutral", children, className }: StateMarkProps) {
  return (
    <span className={cn("inline-flex items-center gap-1.5 font-mono text-xs", className)}>
      <span aria-hidden className={cn("h-3 w-0.5 shrink-0 rounded-full", tones[tone].bar)} />
      <span className={tones[tone].text}>{children}</span>
    </span>
  );
}
