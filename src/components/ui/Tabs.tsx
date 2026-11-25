import type { ReactNode } from "react";

import { cn } from "./cn";

export interface TabItem {
  id: string;
  label: string;
  count?: number;
}

interface TabsProps {
  items: TabItem[];
  value: string;
  onChange: (id: string) => void;
  /** 与标签同一行、靠右的补充信息，例如计数或筛选下拉。 */
  trailing?: ReactNode;
  className?: string;
}

export function Tabs({ items, value, onChange, trailing, className }: TabsProps) {
  return (
    <div className={cn("flex items-end justify-between gap-3 border-b border-border", className)}>
      <div role="tablist" className="flex items-center gap-1">
        {items.map((item) => {
          const active = item.id === value;
          return (
            <button
              key={item.id}
              role="tab"
              type="button"
              aria-selected={active}
              onClick={() => onChange(item.id)}
              className={cn(
                "relative flex h-9 items-center gap-1.5 rounded-t-md px-3 text-sm transition-colors outline-none",
                "focus-visible:ring-3 focus-visible:ring-ring/50",
                active ? "text-foreground" : "text-muted-foreground hover:text-foreground",
              )}
            >
              {item.label}
              {item.count !== undefined ? (
                <span className="font-mono text-xs text-muted-foreground">{item.count}</span>
              ) : null}
              {active ? (
                <span className="absolute inset-x-2 -bottom-px h-[2px] rounded-full bg-link" />
              ) : null}
            </button>
          );
        })}
      </div>
      {trailing ? <div className="flex items-center gap-2 pb-1.5">{trailing}</div> : null}
    </div>
  );
}
