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

export function describeOrigin(task: Task): string {
  if (!task.originKind) return "手动创建";
  return originLabels[task.originKind] ?? task.originKind;
}

/** 列表行只显示备注首行，完整备注在详情里看。 */
export function taskNotePreview(task: Task): string {
  const note = task.note?.trim();
  if (!note) return "没有备注";
  return note.split("\n")[0];
}
