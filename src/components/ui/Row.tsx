import type { ButtonHTMLAttributes, ReactNode } from "react";

import { cn } from "./cn";

interface RowProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  active?: boolean;
  leading?: ReactNode;
  trailing?: ReactNode;
}

/** 导航行：固定 36px 高，hover 用 accent 底，激活为蓝字 + 左侧 2px 竖条。 */
export function Row({
  active = false,
  leading,
  trailing,
  className,
  children,
  ...props
}: RowProps) {
  return (
    <button
      type="button"
      data-active={active || undefined}
      className={cn(
        "group relative flex h-9 w-full items-center gap-2.5 rounded-md px-3 text-left text-sm transition-colors",
        "focus-visible:ring-3 focus-visible:ring-ring/50 outline-none",
        active
          ? "font-medium text-link before:absolute before:inset-y-1.5 before:left-0 before:w-0.5 before:rounded-full before:bg-link"
          : "text-muted-foreground hover:bg-accent/60 hover:text-foreground",
        className,
      )}
      {...props}
    >
      {leading}
      <span className="min-w-0 flex-1 truncate">{children}</span>
      {trailing}
    </button>
  );
}
