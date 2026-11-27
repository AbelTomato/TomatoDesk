import { describe, expect, it } from "vitest";

import {
  emptySelection,
  isEmpty,
  isSelected,
  selectScope,
  selectVisible,
  selectionCount,
  summary,
  toggle,
} from "./inboxSelection";

describe("inbox selection", () => {
  it("toggles a single id without touching the others", () => {
    const checked = toggle(emptySelection, "m1", ["m1", "m2"]);
    expect(checked).toEqual({ mode: "ids", ids: ["m1"] });
    expect(isSelected(checked, "m1")).toBe(true);
    expect(isSelected(checked, "m2")).toBe(false);

    const unchecked = toggle(checked, "m1", ["m1", "m2"]);
    expect(unchecked).toEqual({ mode: "ids", ids: [] });
    expect(isEmpty(unchecked)).toBe(true);
  });

  it("selects the whole page at once and keeps the order unique", () => {
    const selection = selectVisible(["m2", "m1", "m2"]);
    expect(selection).toEqual({ mode: "ids", ids: ["m2", "m1"] });
    expect(selectionCount(selection)).toBe(2);
    expect(summary(selection)).toBe("已选 2 条");
  });

  it("falls back to explicit ids when one row is unchecked in scope mode", () => {
    const all = selectScope(137);
    expect(selectionCount(all)).toBe(137);
    expect(isSelected(all, "anything")).toBe(true);
    expect(summary(all)).toBe("已选全部 137 条");

    const narrowed = toggle(all, "m2", ["m1", "m2", "m3"]);
    expect(narrowed).toEqual({ mode: "ids", ids: ["m1", "m3"] });
    expect(selectionCount(narrowed)).toBe(2);
  });

  it("treats an empty scope as nothing selected", () => {
    expect(isEmpty(selectScope(0))).toBe(true);
    expect(summary(emptySelection)).toBe("已选 0 条");
  });
});
