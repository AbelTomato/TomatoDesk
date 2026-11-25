import type { HTMLAttributes } from "react";

import { cn } from "./cn";

export type BadgeTone = "neutral" | "outline" | "success" | "warning" | "destructive";

const tones: Record<BadgeTone, string> = {
  neutral: "bg-muted text-muted-foreground",
  outline: "border border-border text-muted-foreground",
  success: "text-success",
  warning: "text-warning",
  destructive: "text-destructive",
};

interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  tone?: BadgeTone;
}

/** 标签：小号等宽大写文字，只有中性底或描边，不做彩色填充。 */
export function Badge({ tone = "neutral", className, ...props }: BadgeProps) {
  return (
    <span
      className={cn(
        "inline-flex shrink-0 items-center gap-1 rounded px-1.5 py-0.5 font-mono text-xs uppercase leading-4 tracking-wider",
        tones[tone],
        className,
      )}
      {...props}
    />
  );
}
