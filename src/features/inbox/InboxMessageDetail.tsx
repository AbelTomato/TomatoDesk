import { Badge, Button } from "../../components/ui";
import type { InboxMessage, InboxSource } from "../../lib/api";
import { formatDate, formatRelative } from "../../lib/format";
import { describeCategory, isEmailMessage, sourceKindLabels, triageLabels } from "./shared";

interface InboxMessageDetailProps {
  message: InboxMessage | null;
  source: InboxSource | null;
  mutating: boolean;
  onToggleRead: () => void;
  onDismiss: () => void;
  onConvert: () => void;
  /** 已处理的消息可以撤销，回到待处理；撤销后待办会保留。 */
  onUndo: () => void;
  onOpenLink: () => void;
  /** 命中本地已同步的线程时为回调，否则为 null（只能走 GitHub）。 */
  onOpenLocal: (() => void) | null;
}

/** 收件箱详情：原文只读，动作只有已读、忽略与转为待办。 */
function InboxMessageDetail({
  message,
  source,
  mutating,
  onToggleRead,
  onDismiss,
  onConvert,
  onUndo,
  onOpenLink,
  onOpenLocal,
}: InboxMessageDetailProps) {
  // 邮件没有 GitHub 线程，也不造 mailto 链接，回复交给邮箱客户端。
  const isEmail = message ? isEmailMessage(source, message.category) : false;

  return (
    <section className="min-w-0 overflow-y-auto bg-card px-5 py-6 sm:px-7" aria-label="消息详情">
      {message ? (
        <div className="mx-auto max-w-2xl">
          <div className="flex flex-wrap items-center gap-2">
            <Badge tone="outline">{describeCategory(message.category)}</Badge>
            {message.readAt ? null : <Badge tone="warning">未读</Badge>}
            {message.triageOutcome ? (
              <Badge tone={message.triageOutcome === "dismissed" ? "neutral" : "success"}>
                {triageLabels[message.triageOutcome]}
              </Badge>
            ) : null}
          </div>

          <h3 className="mt-2 text-xl leading-snug font-bold tracking-tight break-words">
            {message.title}
          </h3>

          <p className="mt-2 font-mono text-xs text-muted-foreground">
            {message.actor ?? "未知来源"} ·{" "}
            {source ? `${source.name} · ${sourceKindLabels[source.kind]}` : "未知消息源"} ·{" "}
            {formatDate(message.occurredAt)}
          </p>

          <div className="mt-6 space-y-5">
            {message.summary ? (
              <div>
                <p className="text-xs font-medium text-muted-foreground">摘要</p>
                <p className="mt-2 text-sm leading-7 whitespace-pre-wrap">{message.summary}</p>
              </div>
            ) : null}

            <div>
              <p className="text-xs font-medium text-muted-foreground">原文</p>
              {message.body?.trim() ? (
                <p className="mt-2 rounded-md bg-background px-3 py-2.5 font-mono text-xs leading-6 break-words whitespace-pre-wrap">
                  {message.body}
                </p>
              ) : (
                <p className="mt-2 text-sm text-muted-foreground">
                  {isEmail ? "这封邮件没有可读正文。" : "这条通知没有正文。"}
                </p>
              )}
            </div>

            {message.url ? (
              <div>
                <p className="text-xs font-medium text-muted-foreground">链接</p>
                <p className="mt-2 rounded-md bg-background px-3 py-2 font-mono text-xs break-all text-muted-foreground">
                  {message.url}
                </p>
              </div>
            ) : null}

            {isEmail ? (
              <div>
                <p className="text-xs font-medium text-muted-foreground">相关</p>
                <p className="mt-1 text-xs leading-5 text-muted-foreground">回复请到邮箱客户端。</p>
              </div>
            ) : onOpenLocal || message.url ? (
              <div>
                <p className="text-xs font-medium text-muted-foreground">相关</p>
                <p className="mt-1 text-xs leading-5 text-muted-foreground">
                  {onOpenLocal
                    ? "已关联本地线程，可看正文与评论。"
                    : "本地还没有这条线程，只能打开 GitHub 查看。"}
                </p>
                <div className="mt-2 flex flex-wrap gap-2">
                  {onOpenLocal ? (
                    <Button size="sm" variant="outline" disabled={mutating} onClick={onOpenLocal}>
                      在本地查看线程
                    </Button>
                  ) : null}
                  {message.url ? (
                    <Button size="sm" variant="ghost" disabled={mutating} onClick={onOpenLink}>
                      在 GitHub 打开
                    </Button>
                  ) : null}
                </div>
              </div>
            ) : null}
          </div>

          <div className="mt-8 border-t border-border pt-5">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <p className="text-xs font-medium text-muted-foreground">处理</p>
              {message.triagedAt ? (
                <span className="font-mono text-xs text-muted-foreground">
                  {message.triageOutcome ? `${triageLabels[message.triageOutcome]} · ` : null}
                  {formatRelative(message.triagedAt)}
                </span>
              ) : null}
            </div>

            {message.triagedAt ? (
              <div className="mt-2">
                <Button variant="outline" size="sm" disabled={mutating} onClick={onUndo}>
                  撤销
                </Button>
              </div>
            ) : (
              <div className="mt-2 flex flex-wrap gap-2">
                <Button size="sm" disabled={mutating} onClick={onConvert}>
                  转为待办
                </Button>
                <Button variant="outline" size="sm" disabled={mutating} onClick={onDismiss}>
                  忽略
                </Button>
              </div>
            )}

            <div className="mt-3 flex flex-wrap items-center gap-3">
              <Button variant="ghost" size="sm" disabled={mutating} onClick={onToggleRead}>
                {message.readAt ? "标为未读" : "标为已读"}
              </Button>
              {message.readAt ? (
                <span className="font-mono text-xs text-muted-foreground">
                  读于 {formatDate(message.readAt)}
                </span>
              ) : null}
            </div>
          </div>
        </div>
      ) : (
        <div className="flex min-h-72 items-center justify-center text-center">
          <div>
            <p className="text-sm">选择一条消息查看详情</p>
          </div>
        </div>
      )}
    </section>
  );
}

export default InboxMessageDetail;
