import type { BadgeTone } from "../../components/ui";
import type { Task, TaskStatus } from "../../lib/api";

export const taskStatusLabels: Record<TaskStatus, string> = {
  open: "进行中",
  done: "已完成",
  cancelled: "已取消",
};

export const taskStatusTones: Record<TaskStatus, BadgeTone> = {
  open: "outline",
  done: "success",
  cancelled: "neutral",
};

const originLabels: Record<string, string> = {
  manual: "手动创建",
  inbox: "来自收件箱",
  capture: "来自随手记",
};

/** 收件箱待办再往前追一层：是哪个连接器送进收件箱的。 */
const inboxOriginLabels: Record<string, string> = {
  github: "来自 GitHub 通知",
  email: "来自邮件",
  rss: "来自 RSS",
  manual: "来自收件箱",
};

export function describeOrigin(task: Task): string {
  if (task.originKind === "inbox" && task.originSourceKind) {
    return inboxOriginLabels[task.originSourceKind] ?? "来自收件箱";
  }
  if (!task.originKind) return "手动创建";
  return originLabels[task.originKind] ?? task.originKind;
}

/** 列表行只显示备注首行，完整备注在详情里看。 */
export function taskNotePreview(task: Task): string {
  const note = task.note?.trim();
  if (!note) return "没有备注";
  return note.split("\n")[0];
}
