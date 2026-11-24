# 实施计划：TomatoDesk GitHub CI/CD 与 v0.1.0 发布

- 状态：进行中
- 创建日期：2026-11-24
- 最后更新：2026-11-24
- 负责人：Codex

## 目标

整理并提交当前 TomatoDesk MVP 工作区改动，建立 Windows GitHub Actions 检查与标签发布流程，并发布 GitHub Release `v0.1.0`。

## 背景与现状

- 当前位于 `main`，本地已有 1 个领先 `origin/main` 的提交及一批未提交的 MVP 实现文件。
- 前端和 Tauri 配置版本均为 `0.1.0`；远端尚无 `v0.1.0` 标签或 Release。
- 仓库已有 `scripts/build-installer.ps1`，可生成 NSIS 安装包、SHA-256 和 manifest；仓库尚无 GitHub Actions workflow。
- 本地没有 `gh` CLI；Git 使用 Windows Credential Manager，可用于推送 Git 引用。Release 通过 GitHub Actions 的 `GITHUB_TOKEN` 发布。
- 安装包未签名，步骤 H 的升级安装验证仍未完成；本计划记录这两项限制，不在本次追加签名证书或升级安装验收。

## 范围

### 包含

- 审核并整理当前工作区改动，修复检查发现的局部格式问题。
- 将当前 MVP 代码和文档作为独立提交保留，再增加 CI 与 Release workflow 提交。
- 在 Windows runner 上执行前端 lint/build、Rust 格式/测试/check/Clippy。
- 仅接受与应用配置版本匹配的 `v*` 标签，构建 NSIS，验证 manifest 与 SHA-256 后发布 Release。
- 更新 README、实施计划和当日开发日志。
- 推送 `main` 与 `v0.1.0` 标签，并核验远端 Release 和发布资产。

### 不包含

- 代码签名、自动更新器、其他操作系统安装包。
- 步骤 H 尚未完成的升级安装人工验收。
- 不相关的功能重构或格式化。

## 实施步骤

1. [x] 审核现有变更、清理检查发现的问题，并运行提交前验证。
2. [x] 提交当前 MVP 实现与既有文档改动。
3. [x] 添加 Windows CI、标签 Release workflows，并更新发布说明。
4. [x] 验证 YAML、前端、Rust 和 NSIS 构建，提交 CI/CD 改动。
5. [ ] 推送 `main` 和 `v0.1.0`，确认 GitHub Actions 成功并核验 Release 资产。

## 涉及文件与模块

- `.github/workflows/ci.yml`：PR 与 `main` 推送的 Windows 检查。
- `.github/workflows/release.yml`：版本标签校验、NSIS 构建、校验和及 GitHub Release 发布。
- `docs/releases/v0.1.0.md`：`v0.1.0` 的发布说明，由 Release workflow 作为 Release 正文发布。
- `README.md`：CI 检查和版本发布说明。
- `scripts/build-installer.ps1`：复用既有安装包与 manifest 构建逻辑。
- `docs/execution-plans/`、`docs/development-logs/`：记录范围、状态、验证与未完成限制。

## 验证方式

- `git diff --check` 和 workflow YAML 语法检查。
- `pnpm install --frozen-lockfile`、`pnpm lint`、`pnpm build`。
- Windows Cargo `fmt --check`、`test --locked`、`check --locked`、`clippy --locked`。
- 运行 `scripts/build-installer.ps1` 并核对版本、提交号、manifest、安装包和 SHA-256。
- 推送后检查 CI 和 Release workflow 运行状态、Release 标签与资产列表。

## 风险与回滚

- 风险：Windows runner 依赖、NSIS 安装或外部 GitHub Action 可能导致首次流水线失败；未签名安装包可能触发 Windows SmartScreen 提示。
- 风险：`v0.1.0` 发布后成为公开版本；升级安装尚无验证结果。
- 回滚：若构建失败，不发布 Release，修正 workflow 后重新运行；若发布后发现问题，保留标签历史并撤下 Release 资产，修复后另发补丁版本，不移动已公开标签。

## 完成标准

- [ ] 当前 MVP 与 CI/CD 改动均已通过相应验证并分别提交。
- [ ] `main` 和 `v0.1.0` 已推送，GitHub Release 已发布安装包、SHA-256 和 manifest。
- [ ] README、实施计划和开发日志准确记录结果及未完成的升级安装验证。

## 进度记录

- 2026-11-24 18:28：确认分支、改动清单、版本号、远端 Release/tag 和 Git 凭据情况；仓库未发现现成 CI workflow。
- 2026-11-24 18:38：完成当前 MVP 改动审阅和提交前验证；共享 Cargo target 缺少 SQLite build 输出，使用独立 Windows 临时 target 后 Rust 测试 9/9、`check` 与 Clippy 通过；前端安装、lint、build 和 Rust 格式检查通过。
- 2026-11-24 18:39：当前 MVP 实现与文档已提交为 `2f33633`（`feat: implement core TomatoDesk workflows`）。
- 2026-11-24 22:45：新增 `.github/workflows/ci.yml`、`release.yml` 与 `docs/releases/v0.1.0.md`。核对四个外部 Action 的固定 SHA 与输入参数；workflow YAML 解析通过；本地 `pnpm lint`、`pnpm build` 通过。Release 正文改为读取 `docs/releases/<标签>.md`，缺失时回退到默认说明，避免正文与标签不一致。NSIS 构建改由 Release workflow 在 Windows runner 上验证。
