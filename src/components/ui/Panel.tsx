import type { HTMLAttributes } from "react";

import { cn } from "./cn";

interface PanelProps extends HTMLAttributes<HTMLDivElement> {
  /** 浮层、吸顶栏等需要透出背景的场景才开启，列表与内容区保持不透明。 */
  blur?: boolean;
  inset?: "none" | "sm" | "md" | "lg";
}

const insets = {
  none: "",
  sm: "p-3",
  md: "p-4",
  lg: "p-5 sm:p-6",
};

export function Panel({ blur = false, inset = "none", className, ...props }: PanelProps) {
  return (
    <div
      className={cn(
        "rounded-xl border border-border",
        blur ? "bg-card/72 ring-1 ring-foreground/10 backdrop-blur-xl" : "bg-card",
        insets[inset],
        className,
      )}
      {...props}
    />
  );
}

export function PanelHeader({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn(
        "flex items-start justify-between gap-3 border-b border-border px-4 py-3",
        className,
      )}
      {...props}
    />
  );
}

export function PanelTitle({ className, ...props }: HTMLAttributes<HTMLHeadingElement>) {
  return <h2 className={cn("text-sm font-medium text-foreground", className)} {...props} />;
}

export function PanelBody({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return <div className={cn("px-4 py-3", className)} {...props} />;
}
