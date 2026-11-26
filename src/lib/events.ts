/** 随手记集合发生变化（归档、标签、新建、仓库笔记）时通知外层刷新角标。 */
export const CAPTURES_CHANGED_EVENT = "tomatodesk:captures-changed";

export function notifyCapturesChanged(): void {
  window.dispatchEvent(new Event(CAPTURES_CHANGED_EVENT));
}

export function onCapturesChanged(handler: () => void): () => void {
  window.addEventListener(CAPTURES_CHANGED_EVENT, handler);
  return () => window.removeEventListener(CAPTURES_CHANGED_EVENT, handler);
}

/** 待办新建、编辑或状态变化；收件箱转待办时也会发一次。 */
export const TASKS_CHANGED_EVENT = "tomatodesk:tasks-changed";

export function notifyTasksChanged(): void {
  window.dispatchEvent(new Event(TASKS_CHANGED_EVENT));
}

export function onTasksChanged(handler: () => void): () => void {
  window.addEventListener(TASKS_CHANGED_EVENT, handler);
  return () => window.removeEventListener(TASKS_CHANGED_EVENT, handler);
}

/** 收件箱的已读、忽略与判断结果变化。 */
export const INBOX_CHANGED_EVENT = "tomatodesk:inbox-changed";

export function notifyInboxChanged(): void {
  window.dispatchEvent(new Event(INBOX_CHANGED_EVENT));
}

export function onInboxChanged(handler: () => void): () => void {
  window.addEventListener(INBOX_CHANGED_EVENT, handler);
  return () => window.removeEventListener(INBOX_CHANGED_EVENT, handler);
}
