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
  className?: string;
}

export function Tabs({ items, value, onChange, className }: TabsProps) {
  return (
    <div role="tablist" className={cn("flex items-center gap-1 border-b border-border", className)}>
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
              "relative flex h-9 items-center gap-1.5 px-3 text-sm transition-colors outline-none",
              "focus-visible:ring-3 focus-visible:ring-ring/50 rounded-t-md",
              active ? "text-foreground" : "text-muted-foreground hover:text-foreground",
            )}
          >
            {item.label}
            {item.count !== undefined ? (
              <span className="font-mono text-[11px] text-muted-foreground">{item.count}</span>
            ) : null}
            {active ? (
              <span className="absolute inset-x-2 -bottom-px h-[3px] rounded-full bg-link" />
            ) : null}
          </button>
        );
      })}
    </div>
  );
}
