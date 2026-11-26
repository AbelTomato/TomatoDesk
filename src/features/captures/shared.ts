import type { CaptureKind, CaptureSummary } from "../../lib/api";

export const kindLabels: Record<CaptureKind, string> = {
  note: "笔记",
  link: "链接",
};

/** 列表与详情共用同一套标题回退顺序，避免两处显示不一致。 */
export function getCaptureLabel(capture: CaptureSummary) {
  return capture.title?.trim() || capture.summary?.trim() || capture.note?.trim() || capture.source;
}

export function describeCapture(capture: CaptureSummary) {
  if (capture.summary?.trim()) return capture.summary.trim();
  if (capture.note?.trim()) return capture.note.trim();
  return capture.url || capture.source;
}
