import { Badge, Button } from "../../components/ui";
import type { InboxMessage, InboxSource } from "../../lib/api";
import { formatDate, formatRelative } from "../../lib/format";
import { describeCategory, sourceKindLabels, triageLabels } from "./shared";

interface InboxMessageDetailProps {
  message: InboxMessage | null;
  source: InboxSource | null;
  mutating: boolean;
  onToggleRead: () => void;
  onDismiss: () => void;
  onConvert: () => void;
  onOpenLink: () => void;
}

/** 收件箱详情：原文只读，动作只有已读、忽略与转为待办。 */
function InboxMessageDetail({
  message,
  source,
  mutating,
  onToggleRead,
  onDismiss,
  onConvert,
  onOpenLink,
}: InboxMessageDetailProps) {
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
                  这条通知没有正文，摘要即全部内容。
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
          </div>

          <div className="mt-8 border-t border-border pt-5">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <p className="text-xs font-medium text-muted-foreground">判断</p>
              {message.triagedAt ? (
                <span className="font-mono text-xs text-muted-foreground">
                  {formatRelative(message.triagedAt)}已判断
                </span>
              ) : (
                <span className="font-mono text-xs text-muted-foreground">阅读与判断是两件事</span>
              )}
            </div>

            {message.triagedAt ? (
              <p className="mt-3 text-sm text-muted-foreground">
                这条消息已经判断过，判断结果不会被后续同步覆盖。
              </p>
            ) : (
              <div className="mt-2 flex flex-wrap gap-2">
                <Button size="sm" disabled={mutating} onClick={onConvert}>
                  转为待办
                </Button>
                <Button variant="outline" size="sm" disabled={mutating} onClick={onDismiss}>
                  忽略
                </Button>
                {message.url ? (
                  <Button variant="ghost" size="sm" disabled={mutating} onClick={onOpenLink}>
                    打开原链接
                  </Button>
                ) : null}
              </div>
            )}

            <div className="mt-3 flex flex-wrap items-center gap-3">
              <Button variant="ghost" size="sm" disabled={mutating} onClick={onToggleRead}>
                {message.readAt ? "标记未读" : "标记已读"}
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
            <p className="mt-1 text-sm text-muted-foreground">
              读完可以先放着，判断留到你决定要做什么的时候。
            </p>
          </div>
        </div>
      )}
    </section>
  );
}

export default InboxMessageDetail;
