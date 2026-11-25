# Git 规范

本文件约定 TomatoDesk 仓库的分支、提交、标签、PR 和提交前检查。涉及 `commit`、`push`、`tag`、PR 的操作前先读这里。项目背景见 `README.md`，构建与发布流程见 `docs/development.md`。

## 分支

- `main` 是唯一长期分支，历史保持线性，不使用 merge commit。
- 小改动（文档、单点修复、版本号调整）可直接提交到 `main`。
- 涉及新功能、多文件重构或需要评审的改动，使用短生命周期分支，命名为 `<type>/<简短主题>`，例如 `feat/github-sync`、`fix/inbox-search`，合并后删除。
- 分支基于最新 `main`；同步上游用 `git rebase origin/main`，不要用 `git merge main` 制造合并提交。

## 提交信息

格式：

```text
<type>(<scope>): <subject>

<body>
```

- `type` 取值：`feat`、`fix`、`docs`、`style`、`refactor`、`perf`、`test`、`build`、`ci`、`chore`、`revert`。
  - `feat`：新增功能；`fix`：修复缺陷；`docs`：仅文档；`style`：仅格式（含 rustfmt、prettier）；`refactor`：不改变行为的重构；`perf`：性能；`test`：测试；`build`：依赖或构建配置；`ci`：workflow；`chore`：杂项维护。
- `scope` 可选，小写，取模块名，例如 `db`、`github`、`inbox`、`capture`、`ci`、`docs`、`tauri`。历史提交多不带 scope，只有确实能缩小范围时再加。
- `subject` 用英文祈使句，首字母小写，不以句号结尾，不超过 72 个字符。
- `body` 与 `subject` 之间空一行，说明“为什么改”和“影响范围”，不要逐条复述 diff。现有提交多为单行长句，不强制硬换行。
- 破坏性变更使用 `feat!:` / `fix!:`，或在 body 中写 `BREAKING CHANGE:`。
- 提交信息用英文；开发日志、实施计划和 `docs/` 正文用中文。

本仓库历史示例：

```text
feat: implement core TomatoDesk workflows
ci: add Windows validation and tagged release workflows
style: apply rustfmt to markdown heading helper
docs: record v0.1.0 release verification results
```

## 提交粒度

- 一个提交只做一件事；功能改动与无关的格式化不要混在一起，纯格式化单独用 `style:`。
- 代码与其直接相关的文档（实施计划、开发日志）放在同一提交或紧随其后的提交。
- 不提交构建产物和本地文件：`artifacts/`、`dist/`、`node_modules/`、`*.log`。
- 换行符交给 Git 处理（`core.autocrlf=true`），不要为 CRLF/LF 差异单独制造提交。
- 只暂存本任务涉及的文件，不要用 `git add -A` 扫入无关改动。

## 提交前检查

按改动范围执行，通过后再提交：

- 前端：`pnpm lint`、`pnpm build`、`pnpm format:check`。
- Rust：通过 `scripts/windows-cargo.ps1` 执行 `fmt`、`test`、`check`、`clippy`，不要用 WSL Cargo 代替 Windows MSVC 工具链。
- 空白与冲突标记：`git diff --check`。
- 提交前用 `git status`、`git diff --staged` 确认没有误加入的文件、调试代码或密钥。

## 版本与标签

- 版本以 `package.json` 和 `src-tauri/tauri.conf.json` 为准，发布前两者必须一致。
- 标签格式为 `vX.Y.Z`，必须与上述版本一致；Release workflow 会校验，不一致直接失败。
- 发布说明写入 `docs/releases/<标签>.md`，由 Release workflow 作为 Release 正文发布。
- 已公开的标签不移动、不重打、不删除；发现问题时发布补丁版本。
- 发布细节见 `docs/development.md` 的“CI 与 GitHub Release”。

## Pull Request

- 标题遵循提交信息规范。
- 描述至少包含：变更内容、验证方式与结果、风险与回滚方式。
- 目标分支为 `main`，CI 全部通过后再合并；优先 squash 或 rebase，保持线性历史。

## 禁止事项

- 禁止对 `main` 执行 `git push --force`，禁止强推已公开的标签。
- 禁止改写已推送的提交历史（`--amend`、已推送分支的 `rebase`）；需要修正时追加新提交。
- 禁止用 `--no-verify` 跳过钩子；钩子失败先修根因。
- 禁止提交 Token、密钥、数据库文件、用户数据，以及包含敏感信息的日志或截图。
- GitHub Token 只保存在 Windows Credential Manager，不写入仓库、数据库、日志和提交信息。

## Agent 执行约定

- 涉及 `commit`、`push`、`tag`、PR 前，先读本文件。
- 不要自动提交或推送，除非用户明确要求。
- 提交信息按上述规范用英文书写；完成后按项目规则补当天的开发日志。
