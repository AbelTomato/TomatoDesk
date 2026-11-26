import { memo, useMemo, type ComponentProps } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import rehypeRaw from "rehype-raw";
import rehypeSanitize, { defaultSchema } from "rehype-sanitize";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";

import { openExternal } from "../../lib/openExternal";
import { cn } from "../ui/cn";
import { resolveImageUrl, resolveLinkUrl } from "./markdownUrl";

type MarkdownRendererProps = ComponentProps<typeof ReactMarkdown>;

/**
 * 在 GitHub 默认白名单上再收紧协议：链接只留 http / https / mailto，图片只留 https。
 * 标签与属性沿用默认值（已包含 details / summary / kbd / 任务列表复选框 / language-*）。
 */
const markdownSchema = {
  ...defaultSchema,
  protocols: {
    ...defaultSchema.protocols,
    href: ["http", "https", "mailto"],
    src: ["https"],
    longDesc: ["https"],
  },
  attributes: {
    ...defaultSchema.attributes,
    // 任务列表的勾选状态要保留，否则所有复选框都会显示成未选中。
    input: [
      ["disabled", true],
      ["type", "checkbox"],
      ["checked", true],
    ],
  },
};

const remarkPlugins: MarkdownRendererProps["remarkPlugins"] = [remarkGfm, remarkBreaks];
const rehypePlugins: MarkdownRendererProps["rehypePlugins"] = [
  rehypeRaw,
  [rehypeSanitize, markdownSchema],
];

/** 单条正文的渲染上限：GitHub 本身限制 65k 字符，这里只防异常数据。 */
const MAX_SOURCE_CHARS = 100_000;

function createComponents(baseUrl: string): Components {
  return {
    a({ href, title, children }) {
      const resolved = resolveLinkUrl(href, baseUrl);
      if (!resolved) {
        // 危险协议或无法解析：保留文字，不给可点击入口。
        return <span className="text-muted-foreground">{children}</span>;
      }
      return (
        <a
          href={resolved}
          title={title}
          onClick={(event) => {
            event.preventDefault();
            void openExternal(resolved);
          }}
          // 中键与拖拽同样不能让 WebView 自己导航。
          onAuxClick={(event) => event.preventDefault()}
          onDragStart={(event) => event.preventDefault()}
        >
          {children}
        </a>
      );
    },
    img({ src, alt, title }) {
      const resolved = resolveImageUrl(src, baseUrl);
      if (!resolved) {
        return <span className="text-muted-foreground">[图片：{alt ?? "无法加载"}]</span>;
      }
      return (
        <img
          src={resolved}
          alt={alt ?? ""}
          title={title}
          loading="lazy"
          decoding="async"
          referrerPolicy="no-referrer"
        />
      );
    },
    input({ type, checked }) {
      if (type !== "checkbox") return null;
      // 任务列表只读：GitHub 上的复选框同样不能在这里改。
      return <input type="checkbox" checked={checked ?? false} readOnly disabled />;
    },
    table({ children }) {
      return (
        <div className="my-3 overflow-x-auto">
          <table>{children}</table>
        </div>
      );
    },
  };
}

interface MarkdownProps {
  source: string | null | undefined;
  /** 线程 / 评论的 GitHub 地址，用于解析相对链接与锚点。 */
  baseUrl: string;
  className?: string;
}

/**
 * GitHub 风格 Markdown 渲染。
 * 全链路不产生 innerHTML：react-markdown 输出 React 元素，原始 HTML 先经
 * rehype-raw 解析成 hast，再由 rehype-sanitize 按白名单过滤。
 */
function Markdown({ source, baseUrl, className }: MarkdownProps) {
  const components = useMemo(() => createComponents(baseUrl), [baseUrl]);
  const text = source ?? "";
  const truncated = text.length > MAX_SOURCE_CHARS;
  const content = truncated ? text.slice(0, MAX_SOURCE_CHARS) : text;

  return (
    <div className={cn("markdown", className)}>
      <ReactMarkdown
        remarkPlugins={remarkPlugins}
        rehypePlugins={rehypePlugins}
        components={components}
      >
        {content}
      </ReactMarkdown>
      {truncated ? (
        <p className="mt-3 text-xs text-muted-foreground">内容过长，仅显示前 10 万字符。</p>
      ) : null}
    </div>
  );
}

export default memo(Markdown);
