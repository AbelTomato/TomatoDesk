import { cn } from "./cn";

/** 分节标题：`text-sm font-bold` + 下划线，仓库详情与线程详情共用。 */
function SectionTitle({ className, children }: { className?: string; children: React.ReactNode }) {
  return (
    <h3 className={cn("border-b border-border pb-2 text-sm font-bold tracking-tight", className)}>
      {children}
    </h3>
  );
}

export { SectionTitle };
