import { useEffect, useState } from "react";

import Markdown from "../../components/markdown/Markdown";
import {
  Badge,
  Button,
  Input,
  ListItem,
  ListItemMeta,
  ListItemTitle,
  Panel,
  PanelBody,
  PanelHeader,
  PanelTitle,
  Row,
  StateMark,
  Tabs,
  Textarea,
} from "../../components/ui";

const swatches = [
  { token: "background", className: "bg-background", note: "窗口底色" },
  { token: "card", className: "bg-card", note: "面板" },
  { token: "muted", className: "bg-muted", note: "次级面、hover" },
  { token: "accent", className: "bg-accent", note: "选中态" },
  { token: "primary", className: "bg-primary", note: "主按钮" },
  { token: "link", className: "bg-link", note: "链接、激活" },
  { token: "success", className: "bg-success", note: "成功" },
  { token: "warning", className: "bg-warning", note: "提示" },
  { token: "destructive", className: "bg-destructive", note: "错误、删除" },
  { token: "state-open", className: "bg-state-open", note: "状态：open" },
  { token: "state-merged", className: "bg-state-merged", note: "状态：merged" },
  { token: "state-closed", className: "bg-state-closed", note: "状态：closed" },
];

const typeScale = [
  { label: "text-xs 元信息", className: "font-mono text-xs text-muted-foreground" },
  { label: "text-sm 正文", className: "text-sm" },
  { label: "text-base 强调", className: "text-base font-medium" },
  { label: "text-lg 页面标题", className: "text-lg font-bold tracking-tight" },
  { label: "text-xl 详情标题", className: "text-xl font-bold tracking-tight" },
];

/**
 * Markdown 走查样例：正常语法 + 四个恶意输入。
 * 恶意样例在界面上应表现为纯文字、不可点击、不弹窗。
 */
const markdownSample = [
  "## 改动",
  "",
  "把列表接口换成键集分页，顺手把计数下沉到 SQL。支持 **粗体**、*斜体*、~~删除线~~ 与 `行内代码`。",
  "",
  "- 新增 `list_items_page`，游标格式 `排序值|id`",
  "- 计数走 `count_items_by_status`",
  "",
  "### 待办",
  "",
  "- [x] 迁移脚本",
  "- [ ] 补两千条数据的压测",
  "",
  "| 项目 | 改造前 | 改造后 |",
  "| --- | --- | --- |",
  "| 首屏条数 | 全部 | 50 |",
  "| 计数 | 前端聚合 | SQL |",
  "",
  "```rust",
  "let page = repo.page(&query)?;",
  "```",
  "",
  "> 注意：`open` 筛选包含草稿 PR。",
  "",
  "<details>",
  "<summary>展开看验证记录</summary>",
  "",
  "两千条数据下首屏只取 50 条，滚动时增量加载。",
  "",
  "</details>",
  "",
  "参考 [键集分页](https://example.com/keyset)，或直接看 `src/lib/api.ts`。",
  "",
  "---",
  "",
  "### 恶意样例（应当全部失效）",
  "",
  "<script>alert('xss')</script>",
  "",
  '<img src="https://avatars.githubusercontent.com/u/1?s=120&v=4" onerror="alert(\'xss\')" alt="onerror 应被剥离">',
  "",
  "[危险链接](javascript:alert('xss'))",
  "",
  "![危险图片](data:image/svg+xml;base64,PHN2Zy8+)",
  "",
  '<iframe src="https://example.com"></iframe>',
].join("\n");

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="border-t border-border py-8 first:border-t-0 first:pt-0">
      <h2 className="text-sm font-medium text-muted-foreground">{title}</h2>
      <div className="mt-4">{children}</div>
    </section>
  );
}

function StyleGuide() {
  const [tab, setTab] = useState("overview");
  const [activeRow, setActiveRow] = useState("inbox");

  // 方便直接打开 `?styleguide#markdown` 走查某一段。
  useEffect(() => {
    const id = window.location.hash.slice(1);
    if (!id) return;
    document.getElementById(id)?.scrollIntoView({ block: "start" });
  }, []);

  return (
    <main className="min-h-screen bg-background px-6 py-10 text-foreground">
      <div className="mx-auto max-w-4xl">
        <header className="flex flex-wrap items-end justify-between gap-4 border-b border-border pb-6">
          <div>
            <p className="text-xs text-muted-foreground">开发模式预览</p>
            <h1 className="mt-1 text-lg font-bold tracking-tight">TomatoDesk 设计基座</h1>
          </div>
          <p className="font-mono text-xs text-muted-foreground">
            JetBrains Mono · LXGW WenKai · radius 10px
          </p>
        </header>

        <Section title="色板">
          <div className="grid grid-cols-3 gap-3 sm:grid-cols-5">
            {swatches.map((swatch) => (
              <div key={swatch.token} className="rounded-lg border border-border p-2">
                <div className={`h-10 rounded-md border border-border ${swatch.className}`} />
                <p className="mt-2 font-mono text-xs text-foreground">{swatch.token}</p>
                <p className="text-xs text-muted-foreground">{swatch.note}</p>
              </div>
            ))}
          </div>
        </Section>

        <Section title="排版">
          <div className="space-y-3">
            {typeScale.map((item) => (
              <div key={item.label} className="flex items-baseline justify-between gap-6">
                <span className={`${item.className} text-foreground`}>收集、处理与检索</span>
                <span className="shrink-0 font-mono text-xs text-muted-foreground">
                  {item.label}
                </span>
              </div>
            ))}
            <p className="font-mono text-sm text-muted-foreground">
              0123456789 · #128 · 2026-11-25 15:42
            </p>
          </div>
        </Section>

        <Section title="按钮">
          <div className="flex flex-wrap items-center gap-2">
            <Button>主操作</Button>
            <Button variant="outline">次操作</Button>
            <Button variant="ghost">幽灵</Button>
            <Button variant="destructive">删除</Button>
            <Button disabled>禁用</Button>
          </div>
          <div className="mt-3 flex flex-wrap items-center gap-2">
            <Button size="sm">小号</Button>
            <Button size="md">默认</Button>
            <Button size="lg">大号</Button>
            <Button size="icon" variant="outline" aria-label="设置">
              <span className="text-xs">⚙</span>
            </Button>
          </div>
        </Section>

        <Section title="输入">
          <div className="grid max-w-xl gap-3">
            <Input placeholder="搜索标题、备注和来源" />
            <Input defaultValue="https://github.com/TencentCloud/Octop" />
            <Textarea placeholder="备注（可选）" />
          </div>
        </Section>

        <Section title="徽标">
          <div className="flex flex-wrap items-center gap-2">
            <Badge>待处理 12</Badge>
            <Badge tone="outline">链接</Badge>
            <Badge tone="success">已同步</Badge>
            <Badge tone="warning">未配置</Badge>
            <Badge tone="destructive">同步失败</Badge>
          </div>
        </Section>

        <Section title="列表行与标签页">
          <div className="grid gap-6 sm:grid-cols-2">
            <div className="space-y-1">
              {[
                { id: "inbox", label: "收集", count: 12 },
                { id: "repos", label: "仓库", count: 3 },
                { id: "search", label: "搜索" },
                { id: "settings", label: "设置" },
              ].map((item) => (
                <Row
                  key={item.id}
                  active={activeRow === item.id}
                  onClick={() => setActiveRow(item.id)}
                  trailing={
                    item.count ? (
                      <span className="font-mono text-xs text-muted-foreground">{item.count}</span>
                    ) : null
                  }
                >
                  {item.label}
                </Row>
              ))}
            </div>
            <div>
              <Tabs
                items={[
                  { id: "overview", label: "概览" },
                  { id: "pulls", label: "Pull Requests", count: 8 },
                  { id: "issues", label: "Issues", count: 24 },
                  { id: "reviews", label: "评审", count: 2 },
                  { id: "releases", label: "Release", count: 5 },
                  { id: "items", label: "我的条目", count: 3 },
                ]}
                value={tab}
                onChange={setTab}
              />
              <p className="mt-3 text-sm text-muted-foreground">当前标签页：{tab}</p>
            </div>
          </div>

          <div className="mt-6 divide-y divide-border border-t border-border">
            {[
              {
                id: "1",
                title: "把列表与正文拆成两级加载",
                meta: ["笔记", "待处理", "11-25 15:42"],
              },
              {
                id: "2",
                title: "Octop 的 Issue 同步策略",
                meta: ["链接", "已处理", "11-25 14:10"],
              },
              { id: "3", title: "仓库工作区的信息架构", meta: ["笔记", "已归档", "11-24 22:03"] },
            ].map((item) => (
              <ListItem key={item.id} active={item.id === "1"} onClick={() => {}}>
                <ListItemTitle>{item.title}</ListItemTitle>
                <ListItemMeta>
                  {item.meta.map((entry) => (
                    <span key={entry}>{entry}</span>
                  ))}
                </ListItemMeta>
              </ListItem>
            ))}
          </div>
        </Section>

        <Section title="状态标记（状态色唯一用途）">
          <div className="flex flex-wrap items-center gap-6">
            <StateMark tone="open">open</StateMark>
            <StateMark tone="merged">merged</StateMark>
            <StateMark tone="closed">closed</StateMark>
            <StateMark tone="neutral">draft</StateMark>
          </div>
          <p className="mt-3 text-sm text-muted-foreground">
            2px 竖条 + 小号等宽文字，不做填充徽标；其他位置一律中性色。GitHub
            原生状态沿用其英文小写文案。
          </p>
        </Section>

        <div id="markdown">
          <Section title="Markdown 正文">
            <Markdown
              className="max-w-2xl"
              source={markdownSample}
              baseUrl="https://github.com/TencentCloud/Octop/issues/466"
            />
          </Section>
        </div>

        <Section title="面板">
          <div className="grid gap-4 sm:grid-cols-2">
            <Panel inset="none">
              <PanelHeader>
                <PanelTitle>不透明面板</PanelTitle>
                <Badge tone="outline">默认</Badge>
              </PanelHeader>
              <PanelBody>
                <p className="text-sm text-muted-foreground">
                  内容区与列表使用不透明面板，避免 WebView 里大面积模糊造成掉帧。
                </p>
              </PanelBody>
            </Panel>
            <Panel blur inset="none" className="p-5">
              <p className="text-sm font-medium">半透明面板</p>
              <p className="mt-2 text-sm text-muted-foreground">
                仅用于吸顶栏、浮层与弹窗。这里展示样式，不代表可以在列表里使用。
              </p>
            </Panel>
          </div>
        </Section>
      </div>
    </main>
  );
}

export default StyleGuide;
