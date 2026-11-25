import type { ButtonHTMLAttributes, HTMLAttributes } from "react";

import { cn } from "./cn";

interface ListItemProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  active?: boolean;
  /** 放进 `divide-y` 列表时去掉圆角，由调用处给出水平内边距。 */
  flush?: boolean;
}

/**
 * 列表行：替代卡片列表，靠分隔线成组。
 * 选中用左侧 2px 竖条与浅底色标记，不描边、不加阴影。
 */
export function ListItem({
  active = false,
  flush = false,
  className,
  type = "button",
  ...props
}: ListItemProps) {
  return (
    <button
      type={type}
      data-active={active || undefined}
      className={cn(
        "group relative flex w-full flex-col gap-1.5 text-left transition-colors",
        "hover:bg-accent/60 focus-visible:ring-3 focus-visible:ring-ring/50 outline-none",
        flush ? "rounded-none" : "rounded-md",
        active &&
          "bg-accent/60 before:absolute before:inset-y-2 before:left-0 before:w-0.5 before:rounded-full before:bg-link",
        className,
      )}
      {...props}
    />
  );
}

export function ListItemTitle({
  clamp = 2,
  className,
  ...props
}: HTMLAttributes<HTMLSpanElement> & { clamp?: 1 | 2 }) {
  return (
    <span
      className={cn(
        "text-sm leading-5 font-medium",
        clamp === 1 ? "line-clamp-1" : "line-clamp-2",
        className,
      )}
      {...props}
    />
  );
}

/** 行内元信息：等宽小号、中性色，字段之间用间距而非分隔符。 */
export function ListItemMeta({ className, ...props }: HTMLAttributes<HTMLSpanElement>) {
  return (
    <span
      className={cn(
        "flex flex-wrap items-center gap-x-3 gap-y-1 font-mono text-xs text-muted-foreground",
        className,
      )}
      {...props}
    />
  );
}
