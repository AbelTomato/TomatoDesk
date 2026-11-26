import { useEffect, useState, type FormEvent } from "react";

import { Badge, Button, Input, Textarea } from "../../components/ui";
import type { Task } from "../../lib/api";
import { formatDate } from "../../lib/format";
import { describeOrigin, taskStatusLabels, taskStatusTones } from "./shared";

interface TaskDetailProps {
  task: Task | null;
  mutating: boolean;
  onSave: (title: string, note: string | null) => Promise<void>;
  onComplete: () => void;
  onCancel: () => void;
  onReopen: () => void;
}

/** 待办详情：标题与备注就地编辑，状态动作按当前状态给出可逆的选择。 */
function TaskDetail({ task, mutating, onSave, onComplete, onCancel, onReopen }: TaskDetailProps) {
  const [editing, setEditing] = useState(false);
  const [draftTitle, setDraftTitle] = useState("");
  const [draftNote, setDraftNote] = useState("");
  const [saving, setSaving] = useState(false);

  // 切换待办时退出编辑，避免把上一条的草稿写进这一条。
  useEffect(() => {
    setEditing(false);
  }, [task?.id]);

  function startEditing() {
    if (!task) return;
    setDraftTitle(task.title);
    setDraftNote(task.note ?? "");
    setEditing(true);
  }

  async function submitEdit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const title = draftTitle.trim();
    if (!title || saving) return;
    setSaving(true);
    try {
      await onSave(title, draftNote.trim() ? draftNote : null);
      setEditing(false);
    } catch {
      // 失败时保持编辑态，错误提示由列表页统一展示。
    } finally {
      setSaving(false);
    }
  }

  const busy = mutating || saving;

  return (
    <section className="min-w-0 overflow-y-auto bg-card px-5 py-6 sm:px-7" aria-label="待办详情">
      {task ? (
        <div className="mx-auto max-w-2xl">
          <div className="flex flex-wrap items-start justify-between gap-4">
            <div className="min-w-0 flex-1">
              {editing ? (
                <form className="space-y-3" onSubmit={(event) => void submitEdit(event)}>
                  <label className="block">
                    <span className="sr-only">标题</span>
                    <Input
                      autoFocus
                      value={draftTitle}
                      maxLength={512}
                      onChange={(event) => setDraftTitle(event.target.value)}
                    />
                  </label>
                  <label className="block">
                    <span className="sr-only">备注</span>
                    <Textarea
                      value={draftNote}
                      className="min-h-32"
                      maxLength={16_384}
                      placeholder="补充上下文、链接或下一步"
                      onChange={(event) => setDraftNote(event.target.value)}
                    />
                  </label>
                  <div className="flex items-center gap-2">
                    <Button size="sm" type="submit" disabled={!draftTitle.trim() || saving}>
                      {saving ? "保存中…" : "保存"}
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      disabled={saving}
                      onClick={() => setEditing(false)}
                    >
                      取消
                    </Button>
                  </div>
                </form>
              ) : (
                <>
                  <div className="flex flex-wrap items-center gap-2">
                    <Badge tone={taskStatusTones[task.status]}>
                      {taskStatusLabels[task.status]}
                    </Badge>
                    <span className="font-mono text-xs text-muted-foreground">
                      {describeOrigin(task)}
                    </span>
                  </div>
                  <h3 className="mt-2 text-xl leading-snug font-bold tracking-tight break-words">
                    {task.title}
                  </h3>
                </>
              )}
            </div>
            {!editing ? (
              <Button variant="outline" size="sm" disabled={busy} onClick={startEditing}>
                编辑
              </Button>
            ) : null}
          </div>

          <p className="mt-2 font-mono text-xs text-muted-foreground">
            创建于 {formatDate(task.createdAt)} · 更新于 {formatDate(task.updatedAt)}
            {task.completedAt ? ` · 完成于 ${formatDate(task.completedAt)}` : ""}
          </p>

          {!editing ? (
            <div className="mt-6 space-y-5">
              <div>
                <p className="text-xs font-medium text-muted-foreground">备注</p>
                {task.note?.trim() ? (
                  <p className="mt-2 rounded-md bg-background px-3 py-2.5 text-sm leading-7 whitespace-pre-wrap">
                    {task.note}
                  </p>
                ) : (
                  <p className="mt-2 text-sm text-muted-foreground">还没有备注。</p>
                )}
              </div>
              <div>
                <p className="text-xs font-medium text-muted-foreground">来源</p>
                <p className="mt-2 font-mono text-xs">
                  {describeOrigin(task)}
                  {task.originId ? (
                    <span className="ml-2 text-muted-foreground">{task.originId}</span>
                  ) : null}
                </p>
              </div>
            </div>
          ) : null}

          <div className="mt-8 border-t border-border pt-5">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <p className="text-xs font-medium text-muted-foreground">状态</p>
              {mutating ? (
                <span className="font-mono text-xs text-muted-foreground">保存中…</span>
              ) : (
                <span className="font-mono text-xs text-muted-foreground">完成与取消都可逆</span>
              )}
            </div>
            <div className="mt-2 flex flex-wrap gap-2">
              {task.status === "open" ? (
                <>
                  <Button size="sm" disabled={busy} onClick={onComplete}>
                    完成
                  </Button>
                  <Button variant="outline" size="sm" disabled={busy} onClick={onCancel}>
                    取消待办
                  </Button>
                </>
              ) : (
                <Button variant="outline" size="sm" disabled={busy} onClick={onReopen}>
                  重开
                </Button>
              )}
              {task.status === "done" ? (
                <Button variant="ghost" size="sm" disabled={busy} onClick={onCancel}>
                  改为取消
                </Button>
              ) : null}
            </div>
          </div>
        </div>
      ) : (
        <div className="flex min-h-72 items-center justify-center text-center">
          <div>
            <p className="text-sm">选择一条待办查看详情</p>
            <p className="mt-1 text-sm text-muted-foreground">
              标题与备注在详情里编辑，状态动作在底部。
            </p>
          </div>
        </div>
      )}
    </section>
  );
}

export default TaskDetail;
