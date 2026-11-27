import { useCallback, useEffect, useState } from "react";

import { Button, Input, cn } from "../../components/ui";
import {
  deleteInboxRule,
  listInboxRules,
  listInboxSources,
  previewInboxRule,
  saveInboxRule,
  type InboxRule,
  type InboxRuleAction,
  type InboxRulePreview,
  type InboxSource,
} from "../../lib/api";
import { formatRelative } from "../../lib/format";
import { categoryOptions, describeCategory } from "../inbox/shared";

/** 表单里空字符串表示「不限」，提交时再折成 null。 */
interface RuleForm {
  /** `null` 表示正在新建。 */
  id: string | null;
  sourceId: string;
  category: string;
  actorContains: string;
  titleContains: string;
  action: InboxRuleAction;
}

const emptyForm: RuleForm = {
  id: null,
  sourceId: "",
  category: "",
  actorContains: "",
  titleContains: "",
  action: "dismiss",
};

const actionLabels: Record<InboxRuleAction, string> = {
  read: "标为已读",
  dismiss: "忽略",
};

/** 条件摘要：「GitHub 通知 · 类型「发布」 → 忽略」这类一行能读完的说法。 */
function describeConditions(rule: InboxRule, sources: InboxSource[]): string {
  const parts: string[] = [];
  if (rule.sourceId) {
    const source = sources.find((entry) => entry.id === rule.sourceId);
    // 账户被删掉时规则还在，如实说明来源已不存在，而不是留一片空白。
    parts.push(source ? source.name : "已移除的来源");
  }
  if (rule.category) parts.push(`类型「${describeCategory(rule.category)}」`);
  if (rule.actorContains) parts.push(`触发人含「${rule.actorContains}」`);
  if (rule.titleContains) parts.push(`标题含「${rule.titleContains}」`);
  return parts.join(" · ");
}

/**
 * 收件箱规则：新消息入库时自动标为已读或忽略。
 *
 * 只影响之后进来的消息 —— 已经同步、已经处理过的消息不会被规则回头改写，
 * 否则撤销之后又会被规则收回去。
 */
function InboxRulesSection() {
  const [rules, setRules] = useState<InboxRule[]>([]);
  const [sources, setSources] = useState<InboxSource[]>([]);
  const [form, setForm] = useState<RuleForm>(emptyForm);
  const [formOpen, setFormOpen] = useState(false);
  const [preview, setPreview] = useState<InboxRulePreview | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  const [confirmId, setConfirmId] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      const [nextRules, nextSources] = await Promise.all([listInboxRules(), listInboxSources()]);
      setRules(nextRules);
      setSources(nextSources);
    } catch (loadError) {
      setFailed(true);
      setMessage(loadError instanceof Error ? loadError.message : "加载规则失败，请重试。");
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const titleContains = form.titleContains.trim();
  const actorContains = form.actorContains.trim();
  const hasCondition = Boolean(form.sourceId || form.category || actorContains || titleContains);

  // 试跑：条件变化后延迟一次，边打字边发请求既费流量也会让数字乱跳。
  useEffect(() => {
    if (!formOpen || !hasCondition) {
      setPreview(null);
      return;
    }
    let disposed = false;
    const timer = window.setTimeout(() => {
      void previewInboxRule({
        sourceId: form.sourceId || null,
        category: form.category || null,
        actorContains: actorContains || null,
        titleContains: titleContains || null,
      })
        .then((result) => {
          if (!disposed) setPreview(result);
        })
        .catch(() => {
          if (!disposed) setPreview(null);
        });
    }, 400);
    return () => {
      disposed = true;
      window.clearTimeout(timer);
    };
  }, [actorContains, form.category, form.sourceId, formOpen, hasCondition, titleContains]);

  function openNew() {
    setForm(emptyForm);
    setFormOpen(true);
    setConfirmId(null);
    setMessage(null);
    setFailed(false);
  }

  function openEdit(rule: InboxRule) {
    setForm({
      id: rule.id,
      sourceId: rule.sourceId ?? "",
      category: rule.category ?? "",
      actorContains: rule.actorContains ?? "",
      titleContains: rule.titleContains ?? "",
      action: rule.action,
    });
    setFormOpen(true);
    setConfirmId(null);
    setMessage(null);
    setFailed(false);
  }

  function closeForm() {
    setFormOpen(false);
    setForm(emptyForm);
    setPreview(null);
  }

  async function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (busy || !hasCondition) return;
    setBusy(true);
    setMessage(null);
    setFailed(false);
    const isEdit = form.id !== null;
    try {
      const editing = form.id ? rules.find((rule) => rule.id === form.id) : undefined;
      await saveInboxRule({
        id: form.id,
        sourceId: form.sourceId || null,
        category: form.category || null,
        actorContains: actorContains || null,
        titleContains: titleContains || null,
        action: form.action,
        // 编辑时保留原来的开关状态，避免「改条件」顺手把停用的规则又打开。
        enabled: editing?.enabled ?? true,
      });
      closeForm();
      setMessage(
        isEdit ? "规则已更新，只对之后进来的消息生效。" : "规则已保存，只对之后进来的消息生效。",
      );
      await reload();
    } catch (saveError) {
      setFailed(true);
      setMessage(saveError instanceof Error ? saveError.message : "保存规则失败，请重试。");
    } finally {
      setBusy(false);
    }
  }

  async function toggle(rule: InboxRule) {
    if (busy) return;
    setBusy(true);
    setMessage(null);
    setFailed(false);
    try {
      await saveInboxRule({
        id: rule.id,
        sourceId: rule.sourceId,
        category: rule.category,
        actorContains: rule.actorContains,
        titleContains: rule.titleContains,
        action: rule.action,
        enabled: !rule.enabled,
      });
      setMessage(rule.enabled ? "规则已停用，新消息不再被它处理。" : "规则已启用。");
      await reload();
    } catch (toggleError) {
      setFailed(true);
      setMessage(toggleError instanceof Error ? toggleError.message : "切换规则失败，请重试。");
    } finally {
      setBusy(false);
    }
  }

  async function remove(rule: InboxRule) {
    if (busy) return;
    setBusy(true);
    setMessage(null);
    setFailed(false);
    try {
      setRules(await deleteInboxRule(rule.id));
      setConfirmId(null);
      if (form.id === rule.id) closeForm();
      setMessage("规则已删除，已经处理过的消息不受影响。");
    } catch (removeError) {
      setFailed(true);
      setMessage(removeError instanceof Error ? removeError.message : "删除规则失败，请重试。");
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="py-6">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <h3 className="text-sm font-bold tracking-tight">收件箱规则</h3>
          <p className="mt-1 text-sm leading-6 text-muted-foreground">
            新消息进来时按条件自动标为已读或忽略；已经同步过的消息不受影响。
          </p>
        </div>
        {formOpen ? null : (
          <div className="shrink-0">
            <Button variant="outline" size="sm" disabled={busy} onClick={openNew}>
              新建规则
            </Button>
          </div>
        )}
      </div>

      <div className="mt-3">
        {rules.length === 0 && !formOpen ? (
          <p className="text-sm text-muted-foreground">
            还没有规则。可以按来源、类型或标题把例行通知直接忽略。
          </p>
        ) : null}

        {rules.length > 0 ? (
          <div className="divide-y divide-border border-t border-border">
            {rules.map((rule) => (
              <div key={rule.id} className="py-2.5">
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div className="min-w-0">
                    <p className="text-sm break-words">
                      {describeConditions(rule, sources)}
                      <span aria-hidden className="mx-1.5 text-muted-foreground">
                        →
                      </span>
                      <span className={rule.enabled ? "text-foreground" : "text-muted-foreground"}>
                        {actionLabels[rule.action]}
                      </span>
                    </p>
                    <p className="mt-0.5 font-mono text-xs text-muted-foreground">
                      命中 {rule.hitCount} 次
                      {rule.lastHitAt ? ` · 上次 ${formatRelative(rule.lastHitAt)}` : ""}
                    </p>
                  </div>
                  <div className="flex flex-wrap items-center gap-1">
                    <button
                      type="button"
                      aria-pressed={rule.enabled}
                      className={cn(
                        "h-7 rounded-md border px-2 font-mono text-xs",
                        rule.enabled
                          ? "border-input bg-input/20 text-foreground"
                          : "border-transparent text-muted-foreground hover:text-foreground",
                      )}
                      disabled={busy}
                      onClick={() => void toggle(rule)}
                    >
                      {rule.enabled ? "已启用" : "已停用"}
                    </button>
                    <Button
                      variant="ghost"
                      size="sm"
                      disabled={busy}
                      onClick={() => openEdit(rule)}
                    >
                      编辑
                    </Button>
                    {confirmId === rule.id ? (
                      <>
                        <Button
                          variant="destructive"
                          size="sm"
                          disabled={busy}
                          onClick={() => void remove(rule)}
                        >
                          确认删除
                        </Button>
                        <Button
                          variant="ghost"
                          size="sm"
                          disabled={busy}
                          onClick={() => setConfirmId(null)}
                        >
                          取消
                        </Button>
                      </>
                    ) : (
                      <Button
                        variant="destructive"
                        size="sm"
                        disabled={busy}
                        onClick={() => {
                          setConfirmId(rule.id);
                          setMessage(null);
                        }}
                      >
                        删除
                      </Button>
                    )}
                  </div>
                </div>
              </div>
            ))}
          </div>
        ) : null}

        {formOpen ? (
          <form className="mt-3 space-y-2 border-t border-border pt-3" onSubmit={submit}>
            <div className="flex flex-wrap items-baseline justify-between gap-2">
              <p className="text-xs font-medium text-muted-foreground">
                {form.id ? "编辑规则" : "新建规则"}
              </p>
              <span className="font-mono text-xs text-muted-foreground">
                条件之间是「且」，至少填一项
              </span>
            </div>
            <div className="grid gap-2 sm:grid-cols-2">
              <label className="flex flex-col gap-1">
                <span className="text-xs text-muted-foreground">来源</span>
                <select
                  className="h-8 w-full rounded-lg border border-input bg-transparent px-2.5 text-sm text-foreground outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:cursor-not-allowed disabled:opacity-50"
                  value={form.sourceId}
                  onChange={(event) =>
                    setForm((current) => ({ ...current, sourceId: event.target.value }))
                  }
                  disabled={busy}
                >
                  <option value="">全部来源</option>
                  {sources.map((source) => (
                    <option key={source.id} value={source.id}>
                      {source.name}
                    </option>
                  ))}
                </select>
              </label>
              <label className="flex flex-col gap-1">
                <span className="text-xs text-muted-foreground">类型</span>
                <select
                  className="h-8 w-full rounded-lg border border-input bg-transparent px-2.5 text-sm text-foreground outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:cursor-not-allowed disabled:opacity-50"
                  value={form.category}
                  onChange={(event) =>
                    setForm((current) => ({ ...current, category: event.target.value }))
                  }
                  disabled={busy}
                >
                  <option value="">全部类型</option>
                  {categoryOptions.map((option) => (
                    <option key={option.value} value={option.value}>
                      {option.label}
                    </option>
                  ))}
                </select>
              </label>
              <label className="flex flex-col gap-1">
                <span className="text-xs text-muted-foreground">触发人包含</span>
                <Input
                  value={form.actorContains}
                  onChange={(event) =>
                    setForm((current) => ({ ...current, actorContains: event.target.value }))
                  }
                  placeholder="dependabot"
                  maxLength={256}
                  disabled={busy}
                />
              </label>
              <label className="flex flex-col gap-1">
                <span className="text-xs text-muted-foreground">标题包含</span>
                <Input
                  value={form.titleContains}
                  onChange={(event) =>
                    setForm((current) => ({ ...current, titleContains: event.target.value }))
                  }
                  placeholder="升级"
                  maxLength={512}
                  disabled={busy}
                />
              </label>
              <label className="flex flex-col gap-1">
                <span className="text-xs text-muted-foreground">动作</span>
                <select
                  className="h-8 w-full rounded-lg border border-input bg-transparent px-2.5 text-sm text-foreground outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:cursor-not-allowed disabled:opacity-50"
                  value={form.action}
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      action: event.target.value as InboxRuleAction,
                    }))
                  }
                  disabled={busy}
                >
                  <option value="dismiss">忽略</option>
                  <option value="read">标为已读</option>
                </select>
              </label>
            </div>

            <p className="text-xs leading-5 text-muted-foreground">
              标为已读会把邮件同步到服务器，忽略只改本地状态。规则不会自动转成待办。
            </p>

            <div className="flex flex-wrap items-center gap-2">
              <Button type="submit" disabled={busy || !hasCondition}>
                {form.id ? "保存修改" : "保存规则"}
              </Button>
              <Button variant="ghost" disabled={busy} onClick={closeForm}>
                取消
              </Button>
              {preview ? (
                <span className="font-mono text-xs text-muted-foreground">
                  最近 14 天命中 {preview.matchCount} 条，其中 {preview.pendingCount} 条还没处理
                </span>
              ) : (
                <span className="font-mono text-xs text-muted-foreground">
                  {hasCondition ? "正在试跑…" : "填一项条件后会试跑最近 14 天的消息"}
                </span>
              )}
            </div>

            {preview && preview.sampleTitles.length > 0 ? (
              <ul className="space-y-1 border-l-2 border-border pl-3">
                {preview.sampleTitles.slice(0, 3).map((title, index) => (
                  <li
                    key={`${index}-${title}`}
                    className="line-clamp-1 text-xs text-muted-foreground"
                  >
                    {title}
                  </li>
                ))}
                {preview.sampleTitles.length > 3 ? (
                  <li className="font-mono text-xs text-muted-foreground">…</li>
                ) : null}
              </ul>
            ) : null}
          </form>
        ) : null}

        {message ? (
          <p
            className={cn(
              "mt-2 font-mono text-xs leading-5 break-words",
              failed ? "text-destructive" : "text-success",
            )}
            role={failed ? "alert" : "status"}
          >
            {message}
          </p>
        ) : null}
      </div>
    </section>
  );
}

export default InboxRulesSection;
