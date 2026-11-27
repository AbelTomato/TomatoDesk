import type { ReactNode } from "react";

import { Button, Panel } from "../../components/ui";
import type { InboxAction } from "../../lib/api";
import { actionLabels } from "./shared";
import { selectionCount, summary, type InboxSelection } from "./inboxSelection";

interface BulkActionBarProps {
  selection: InboxSelection;
  /** 当前筛选下还有没加载出来的行：这时才提供第二级全选。 */
  hasMore: boolean;
  /** 「选择全部 N 条」的 N，直接来自列表查询返回的当前筛选总数。 */
  scopeTotal: number;
  busy: boolean;
  onSelectScope: () => void;
  onAction: (action: InboxAction) => void;
  onClear: () => void;
}

const actions: InboxAction[] = ["read", "unread", "dismiss", "task"];

/**
 * 底部操作条：只在选择态出现，浮在列表上方，不挤占行高。
 *
 * 「忽略」刻意不用主按钮样式：它是四件事里唯一不可逆的那个（撤销只有 8 秒）。
 */
function BulkActionBar({
  selection,
  hasMore,
  scopeTotal,
  busy,
  onSelectScope,
  onAction,
  onClear,
}: BulkActionBarProps) {
  const count = selectionCount(selection);
  const canSelectAll = selection.mode === "ids" && hasMore && scopeTotal > count;
  const idle = busy || count === 0;

  return (
    <div className="pointer-events-none fixed inset-x-0 bottom-4 z-40 flex justify-center px-4">
      <Panel
        blur
        className="pointer-events-auto flex flex-wrap items-center gap-x-2 gap-y-1 px-3 py-2"
        role="toolbar"
        aria-label="批量处理"
      >
        <span className="font-mono text-xs text-muted-foreground">{summary(selection)}</span>
        <span aria-hidden className="mx-1 h-4 w-px bg-border" />
        {actions.map((action) => (
          <Button
            key={action}
            size="sm"
            // 转待办不留跨页全选的入口：误转只能逐条手删待办。
            variant={action === "task" ? "default" : "ghost"}
            disabled={idle || (action === "task" && selection.mode === "scope")}
            title={
              action === "task" && selection.mode === "scope"
                ? "转为待办只对勾选的消息生效"
                : actionLabels[action]
            }
            onClick={() => onAction(action)}
          >
            {actionLabels[action]}
          </Button>
        ))}
        {canSelectAll ? (
          <Button size="sm" variant="outline" disabled={busy} onClick={onSelectScope}>
            选择全部 {scopeTotal} 条
          </Button>
        ) : null}
        <Button size="sm" variant="ghost" disabled={busy} onClick={onClear}>
          取消
        </Button>
      </Panel>
    </div>
  );
}

interface ConfirmDialogProps {
  title: string;
  description: string;
  /** 需要逐条列出时给前几条，长清单只列开头。 */
  details?: string[];
  confirmLabel: string;
  busy?: boolean;
  children?: ReactNode;
  onConfirm: () => void;
  onCancel: () => void;
}

/** 覆盖层确认框：只用于「说清楚后果」的两处——批量转待办、条数变了。 */
export function ConfirmDialog({
  title,
  description,
  details,
  confirmLabel,
  busy = false,
  children,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 px-6">
      <Panel blur inset="lg" className="w-full max-w-md" role="dialog" aria-modal="true">
        <h2 className="text-base font-bold tracking-tight">{title}</h2>
        <p className="mt-2 text-sm leading-6 text-muted-foreground">{description}</p>
        {details && details.length > 0 ? (
          <ul className="mt-3 space-y-1 border-l-2 border-border pl-3">
            {details.map((detail) => (
              <li key={detail} className="text-sm break-words">
                {detail}
              </li>
            ))}
          </ul>
        ) : null}
        {children}
        <div className="mt-5 flex justify-end gap-2">
          <Button variant="ghost" size="sm" disabled={busy} onClick={onCancel}>
            取消
          </Button>
          <Button size="sm" disabled={busy} onClick={onConfirm}>
            {confirmLabel}
          </Button>
        </div>
      </Panel>
    </div>
  );
}

export default BulkActionBar;
