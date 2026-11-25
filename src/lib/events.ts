/** 条目集合发生变化（状态、标签、收集、仓库笔记）时通知外层刷新角标。 */
export const ITEMS_CHANGED_EVENT = "tomatodesk:items-changed";

export function notifyItemsChanged(): void {
  window.dispatchEvent(new Event(ITEMS_CHANGED_EVENT));
}

export function onItemsChanged(handler: () => void): () => void {
  window.addEventListener(ITEMS_CHANGED_EVENT, handler);
  return () => window.removeEventListener(ITEMS_CHANGED_EVENT, handler);
}
