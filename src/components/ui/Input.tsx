import type { ComponentPropsWithRef } from "react";

import { cn } from "./cn";

const field =
  "w-full min-w-0 rounded-lg border border-input bg-transparent px-2.5 text-sm text-foreground outline-none transition-colors placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:cursor-not-allowed disabled:opacity-50";

export function Input({ className, ...props }: ComponentPropsWithRef<"input">) {
  return <input className={cn(field, "h-8", className)} {...props} />;
}

export function Textarea({ className, ...props }: ComponentPropsWithRef<"textarea">) {
  return (
    <textarea className={cn(field, "min-h-20 resize-y py-1.5 leading-6", className)} {...props} />
  );
}
