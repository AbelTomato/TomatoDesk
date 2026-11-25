import { useState } from "react";

import {
  Badge,
  Button,
  Input,
  Panel,
  PanelBody,
  PanelHeader,
  PanelTitle,
  Row,
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
];

const typeScale = [
  { label: "text-[11px] 元信息", className: "text-[11px]" },
  { label: "text-xs 辅助", className: "text-xs" },
  { label: "text-sm 正文", className: "text-sm" },
  { label: "text-base 强调", className: "text-base" },
  { label: "text-lg 小标题", className: "text-lg font-medium" },
  { label: "text-2xl 页面标题", className: "text-2xl font-medium tracking-tight" },
];

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

  return (
    <main className="min-h-screen bg-background px-6 py-10 text-foreground">
      <div className="mx-auto max-w-4xl">
        <header className="flex flex-wrap items-end justify-between gap-4 border-b border-border pb-6">
          <div>
            <p className="text-xs text-muted-foreground">开发模式预览</p>
            <h1 className="mt-1 text-2xl font-medium tracking-tight">TomatoDesk 设计基座</h1>
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
                <p className="mt-2 font-mono text-[11px] text-foreground">{swatch.token}</p>
                <p className="text-[11px] text-muted-foreground">{swatch.note}</p>
              </div>
            ))}
          </div>
        </Section>

        <Section title="排版">
          <div className="space-y-3">
            {typeScale.map((item) => (
              <div key={item.label} className="flex items-baseline justify-between gap-6">
                <span className={`${item.className} text-foreground`}>收集、处理与检索</span>
                <span className="shrink-0 font-mono text-[11px] text-muted-foreground">
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
                      <span className="font-mono text-[11px] text-muted-foreground">
                        {item.count}
                      </span>
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
                  { id: "pulls", label: "PR", count: 8 },
                  { id: "issues", label: "Issue", count: 24 },
                  { id: "reviews", label: "Reviews", count: 2 },
                ]}
                value={tab}
                onChange={setTab}
              />
              <p className="mt-3 text-sm text-muted-foreground">当前标签页：{tab}</p>
            </div>
          </div>
        </Section>

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
