import type { ButtonHTMLAttributes } from "react";

import { cn } from "./cn";

export type ButtonVariant = "default" | "outline" | "ghost" | "destructive";
export type ButtonSize = "sm" | "md" | "lg" | "icon";

const base =
  "inline-flex shrink-0 select-none items-center justify-center gap-1.5 whitespace-nowrap rounded-lg text-sm font-medium transition-colors outline-none focus-visible:ring-3 focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50 active:translate-y-px";

/**
 * 每个变体自带描边，base 不写 border-* 颜色，避免 `border-transparent`
 * 与变体的 `border-input` 互相覆盖（cn 只做拼接，不处理冲突）。
 */
const variants: Record<ButtonVariant, string> = {
  default: "border border-transparent bg-primary text-primary-foreground hover:bg-primary/90",
  outline:
    "border border-input bg-input/20 text-foreground hover:bg-accent hover:text-accent-foreground",
  ghost: "border border-transparent text-muted-foreground hover:bg-accent hover:text-foreground",
  destructive: "border border-transparent text-destructive hover:bg-destructive/10",
};

const sizes: Record<ButtonSize, string> = {
  sm: "h-7 px-2.5 text-xs",
  md: "h-8 px-2.5",
  lg: "h-9 px-3",
  icon: "size-8",
};

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
}

export function Button({
  variant = "default",
  size = "md",
  className,
  type = "button",
  ...props
}: ButtonProps) {
  return (
    <button
      type={type}
      className={cn(base, variants[variant], sizes[size], className)}
      {...props}
    />
  );
}
