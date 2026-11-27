import type { FormEvent } from "react";

import { Button, cn } from "../../components/ui";
import type { CaptureBody, CaptureSummary, Tag } from "../../lib/api";
import { formatDate } from "../../lib/format";
import { getCaptureLabel, kindLabels } from "./shared";

interface CaptureDetailProps {
  capture: CaptureSummary | null;
  body: CaptureBody | null;
  bodyLoading: boolean;
  bodyError: string | null;
  tags: Tag[];
  selectedTags: Tag[];
  tagSaving: boolean;
  mutating: boolean;
  newTagName: string;
  onNewTagNameChange: (value: string) => void;
  onOpenLink: () => void;
  onToggleArchived: () => void;
  onToggleTag: (tagId: string) => void;
  onAddTag: (event: FormEvent<HTMLFormElement>) => void;
}

/** 随手记详情：正文按需加载，归档是「收起来」而不是删除，所以只有归档 / 取消归档两态。 */
function CaptureDetail({
  capture,
  body,
  bodyLoading,
  bodyError,
  tags,
  selectedTags,
  tagSaving,
  mutating,
  newTagName,
  onNewTagNameChange,
  onOpenLink,
  onToggleArchived,
  onToggleTag,
  onAddTag,
}: CaptureDetailProps) {
  return (
    <section className="min-w-0 overflow-y-auto bg-card px-5 py-6 sm:px-7" aria-label="随手记详情">
      {capture ? (
        <div className="mx-auto max-w-2xl">
          <div className="flex flex-wrap items-start justify-between gap-4">
            <div className="min-w-0">
              <h3 className="text-xl leading-snug font-bold tracking-tight break-words">
                {getCaptureLabel(capture)}
              </h3>
              <p className="mt-1 font-mono text-xs text-muted-foreground">
                {kindLabels[capture.kind]} · 更新于 {formatDate(capture.updatedAt)} · 创建于{" "}
                {formatDate(capture.createdAt)}
              </p>
            </div>
            {capture.url ? (
              <Button size="sm" onClick={onOpenLink}>
                打开原链接
              </Button>
            ) : null}
          </div>

          <div className="mt-6 space-y-5">
            <div>
              <p className="text-xs font-medium text-muted-foreground">正文</p>
              {bodyLoading ? (
                <div className="mt-2 space-y-2" role="status" aria-label="正在加载正文">
                  {Array.from({ length: 3 }).map((_, index) => (
                    <div
                      key={index}
                      className="h-3 animate-pulse rounded bg-muted"
                      style={{ width: `${92 - index * 14}%` }}
                    />
                  ))}
                </div>
              ) : bodyError ? (
                <p className="mt-2 text-sm text-destructive">{bodyError}</p>
              ) : body?.body?.trim() ? (
                <p className="mt-2 text-sm leading-7 whitespace-pre-wrap">{body.body}</p>
              ) : (
                <p className="mt-2 text-sm text-muted-foreground">
                  {capture.kind === "link"
                    ? "没有可显示的正文，可打开原链接查看。"
                    : "这条随手记没有正文。"}
                </p>
              )}
            </div>

            {capture.url ? (
              <div>
                <p className="text-xs font-medium text-muted-foreground">URL</p>
                <p className="mt-2 rounded-md bg-background px-3 py-2 font-mono text-xs break-all text-muted-foreground">
                  {capture.url}
                </p>
              </div>
            ) : null}
            {capture.summary ? (
              <div>
                <p className="text-xs font-medium text-muted-foreground">摘要</p>
                <p className="mt-2 text-sm leading-7 whitespace-pre-wrap">{capture.summary}</p>
              </div>
            ) : null}
            {capture.note ? (
              <div>
                <p className="text-xs font-medium text-muted-foreground">备注</p>
                <p className="mt-2 rounded-md bg-background px-3 py-2.5 text-sm leading-7 whitespace-pre-wrap">
                  {capture.note}
                </p>
              </div>
            ) : null}
            <div>
              <p className="text-xs font-medium text-muted-foreground">来源</p>
              <p className="mt-2 font-mono text-xs">{capture.source}</p>
            </div>
          </div>

          <div className="mt-8 border-t border-border pt-5">
            <div className="flex items-center justify-between gap-3">
              <p className="text-xs font-medium text-muted-foreground">归档</p>
              {mutating ? (
                <span className="font-mono text-xs text-muted-foreground">保存中…</span>
              ) : capture.archivedAt ? (
                <span className="font-mono text-xs text-muted-foreground">
                  归档于 {formatDate(capture.archivedAt)}
                </span>
              ) : null}
            </div>
            <div className="mt-2">
              <Button
                size="sm"
                variant={capture.archivedAt ? "outline" : "default"}
                disabled={mutating}
                onClick={onToggleArchived}
              >
                {capture.archivedAt ? "取消归档" : "归档"}
              </Button>
            </div>
          </div>

          <div className="mt-6 border-t border-border pt-5">
            <div className="flex items-center justify-between gap-3">
              <p className="text-xs font-medium text-muted-foreground">标签</p>
              <span className="font-mono text-xs text-muted-foreground">点击切换</span>
            </div>
            <div className="mt-2 flex flex-wrap gap-1.5">
              {tags.map((tag) => {
                const active = selectedTags.some((selectedTag) => selectedTag.id === tag.id);
                return (
                  <button
                    key={tag.id}
                    className={cn(
                      "h-7 rounded-md border px-2 font-mono text-xs transition-colors",
                      active
                        ? "border-link/50 text-link"
                        : "border-border text-muted-foreground hover:border-foreground/30 hover:text-foreground",
                    )}
                    type="button"
                    disabled={tagSaving}
                    onClick={() => onToggleTag(tag.id)}
                  >
                    {tag.name}
                  </button>
                );
              })}
              {tags.length === 0 ? (
                <span className="text-sm text-muted-foreground">还没有标签</span>
              ) : null}
            </div>
            <form className="mt-3 flex max-w-sm gap-2" onSubmit={onAddTag}>
              <input
                className="h-8 min-w-0 flex-1 rounded-lg border border-input bg-transparent px-2.5 text-sm outline-none placeholder:text-muted-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
                value={newTagName}
                onChange={(event) => onNewTagNameChange(event.target.value)}
                placeholder="新建标签"
                maxLength={128}
              />
              <Button
                variant="outline"
                size="sm"
                type="submit"
                disabled={!newTagName.trim() || tagSaving}
              >
                添加
              </Button>
            </form>
          </div>
        </div>
      ) : (
        <div className="flex min-h-72 items-center justify-center text-center">
          <div>
            <p className="text-sm">选择一条随手记查看详情</p>
          </div>
        </div>
      )}
    </section>
  );
}

export default CaptureDetail;
