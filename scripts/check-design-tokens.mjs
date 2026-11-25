#!/usr/bin/env node
// 设计规范落地检查：旧 token 清零、模糊只出现在浮层组件。
// 用法：pnpm check:design（在仓库根目录运行）。

import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const SOURCE_DIR = "src";

/** 旧 token 与禁用写法，出现即视为未清理干净。 */
const forbidden = [
  { pattern: /tomato-/, note: "旧番茄色 token" },
  { pattern: /\bink-/, note: "旧 ink token" },
  { pattern: /bg-canvas|bg-surface|border-line/, note: "旧语义类名" },
  { pattern: /text-white/, note: "硬编码白色文字" },
  { pattern: /tracking-\[0\.18em\]/, note: "旧眉标字距" },
  {
    pattern: /\bbg-(red|green|blue|purple|amber|yellow|emerald|slate)-(50|100|200)\b/,
    note: "彩色填充底",
  },
];

/** 只允许出现在浮层组件的写法。 */
const scoped = [
  {
    pattern: /backdrop-blur/,
    note: "backdrop-blur 只用于浮层",
    allow: ["src/components/ui/Panel.tsx"],
  },
];

function walk(dir) {
  const entries = readdirSync(dir, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) files.push(...walk(path));
    else if (/\.(tsx?|css)$/.test(entry.name)) files.push(path);
  }
  return files;
}

const violations = [];
for (const file of walk(SOURCE_DIR)) {
  const text = readFileSync(file, "utf8");
  text.split("\n").forEach((line, index) => {
    const normalized = file.replaceAll("\\", "/");
    for (const rule of forbidden) {
      if (rule.pattern.test(line)) {
        violations.push(`${normalized}:${index + 1}  ${rule.note} → ${line.trim()}`);
      }
    }
    for (const rule of scoped) {
      if (!rule.pattern.test(line)) continue;
      // 只看真正写进 className 的地方，注释与文案里的词不算。
      if (!/className=/.test(line)) continue;
      if (rule.allow.some((allowed) => normalized === allowed)) continue;
      violations.push(`${normalized}:${index + 1}  ${rule.note} → ${line.trim()}`);
    }
  });
}

if (violations.length > 0) {
  console.error(`设计规范检查未通过，共 ${violations.length} 处：`);
  for (const item of violations) console.error(`  ${item}`);
  process.exit(1);
}

console.log("设计规范检查通过：无旧 token，模糊只出现在浮层组件。");
