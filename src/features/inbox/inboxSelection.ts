/**
 * 收件箱的选择态：要么是显式勾选的 id，要么是「当前筛选的全部 N 条」。
 *
 * 全部做成纯函数，动作条只负责渲染。`scope` 模式不展开 id —— 几千条 id 存在前端
 * 没有意义，后端按同一套筛选重新解析，前端只负责把确认时的条数带回去对账。
 */
export type InboxSelection = { mode: "ids"; ids: string[] } | { mode: "scope"; total: number };

export const emptySelection: InboxSelection = { mode: "ids", ids: [] };

export function isSelected(selection: InboxSelection, id: string): boolean {
  return selection.mode === "scope" || selection.ids.includes(id);
}

export function selectionCount(selection: InboxSelection): number {
  return selection.mode === "scope" ? selection.total : selection.ids.length;
}

export function isEmpty(selection: InboxSelection): boolean {
  return selectionCount(selection) === 0;
}

/**
 * 勾选或取消一条。
 *
 * `scope` 模式下所有可见项都是勾上的，点掉一条就退回显式勾选：`visibleIds` 用来
 * 还原「除了这条以外都还选着」。少了它，用户点错一条就得从头再选。
 */
export function toggle(
  selection: InboxSelection,
  id: string,
  visibleIds: string[],
): InboxSelection {
  if (selection.mode === "scope") {
    return { mode: "ids", ids: visibleIds.filter((entry) => entry !== id) };
  }
  return {
    mode: "ids",
    ids: selection.ids.includes(id)
      ? selection.ids.filter((entry) => entry !== id)
      : [...selection.ids, id],
  };
}

/** 全选当前页（第一级：只覆盖已经加载出来的行）。 */
export function selectVisible(ids: string[]): InboxSelection {
  return { mode: "ids", ids: [...new Set(ids)] };
}

/** 选择全部 N 条（第二级：切到 `scope` 模式，由后端解析 id）。 */
export function selectScope(total: number): InboxSelection {
  return { mode: "scope", total };
}

export function summary(selection: InboxSelection): string {
  return selection.mode === "scope"
    ? `已选全部 ${selection.total} 条`
    : `已选 ${selection.ids.length} 条`;
}
