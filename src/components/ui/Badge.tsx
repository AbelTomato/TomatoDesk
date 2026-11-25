import type { HTMLAttributes } from "react";

import { cn } from "./cn";

export type BadgeTone = "neutral" | "outline" | "success" | "warning" | "destructive";

const tones: Record<BadgeTone, string> = {
  neutral: "bg-muted text-muted-foreground",
  outline: "border border-border text-muted-foreground",
  success: "bg-success/10 text-success",
  warning: "bg-warning/10 text-warning",
  destructive: "bg-destructive/10 text-destructive",
};

interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  tone?: BadgeTone;
}

export function Badge({ tone = "neutral", className, ...props }: BadgeProps) {
  return (
    <span
      className={cn(
        "inline-flex shrink-0 items-center gap-1 rounded px-1.5 py-0.5 text-[11px] font-medium leading-4",
        tones[tone],
        className,
      )}
      {...props}
    />
  );
}
