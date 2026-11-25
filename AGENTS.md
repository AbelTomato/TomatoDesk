# 项目开发规则

## 文档位置

- 项目文档统一存放在 `docs/`。
- 开发日志统一存放在 `docs/development-logs/`。
- 实施计划统一存放在 `docs/execution-plans/`。
- 新增文档时，优先放入上述目录，不要在项目根目录散落文档。

## 文档索引

按任务需要加载，不要一次性全读：

- `README.md`：项目背景与功能基本介绍。
- `docs/development.md`：开发环境、常用命令、安装包构建、CI 与发布流程。涉及构建、发布或 Windows Cargo 时加载。
- `docs/git-conventions.md`：分支、提交信息、提交前检查、版本标签、PR 和禁止事项。涉及 `commit`、`push`、`tag`、PR 时加载。
- `docs/execution-plans/`：实施计划，开始任务前查看相关计划恢复上下文。
- `docs/development-logs/`：按日期归档的开发日志，查看最近记录恢复上下文。
- `docs/releases/`：版本发布说明，正文由 Release workflow 直接发布。

## README 维护

- 默认不修改 `README.md`；只有用户明确要求修改时才动它。
- `README.md` 只负责存放项目背景与功能基本介绍，不写开发命令、构建步骤、CI 与发布流程、工具链路径等实现细节。
- 开发类内容写入 `docs/`：开发环境、构建与发布流程放 `docs/development.md`，进度与决策放实施计划和开发日志。
- 功能发生变化时，先更新 `docs/`，不要顺手改 README。

## 本机环境与工具链

- 开发前先检测当前环境，看是Windows原生还是 WSL2 Ubuntu；若为WSL，可通过 `/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe` 调用 Windows PowerShell。
- WSL 内的 Linux Rust 工具链位于 `/home/abeltomato/.cargo/bin`，当前 `cargo` 为 1.99.0。
- Windows 端 Rust 工具链位于 `C:\Users\AbelTomato\.cargo\bin`，当前 `cargo`/`rustc` 为 1.99.1，活动 toolchain 为 `stable-x86_64-pc-windows-msvc`。
- Windows 目标的 Rust 检查和构建必须使用 Windows Cargo。优先调用 `scripts/windows-cargo.ps1`，不要直接使用 WSL Cargo 代替 Windows MSVC 工具链。
- 从 WSL 调用 Windows Cargo 时，使用 `wslpath -w` 将仓库路径转换为 Windows UNC 路径；包装脚本会将 `CARGO_TARGET_DIR` 放到 `%TEMP%\TomatoDesk\cargo-target` 并关闭增量编译。
- 不要把 Windows Cargo 的 target 目录放在 `\\wsl.localhost` 文件系统中，否则可能因增量编译锁文件或原子移动失败而构建失败。

需要确认调用方式时，先看 `docs/development.md` 的“从 WSL 调用 Windows Cargo”章节；任务进度和已验证结果看 `docs/execution-plans/tomatodesk-mvp.md` 以及当天的 `docs/development-logs/YYYY-MM-DD.md`。

## 开发日志

- 按日期归档，每天使用一个 `YYYY-MM-DD.md` 文件，例如 `2026-11-23.md`。
- 同一天的记录追加到同一个文件，按时间顺序排列。
- 写入日志前先获取当前上海时间，并使用该时间确定日志文件日期和记录时间。
- 文件第一行使用以下格式：

  `#YYYY-MM-DD开发日志`

- 每条记录使用以下格式作为小节标题：

  `## ##hh:mm:ss-xxxxx`

  其中 `hh:mm:ss` 是 24 小时制，`xxxxx` 是简短的主题或记录标识。

- 日志内容尽可能简洁易懂，只保留模型恢复上下文所需的信息：目标、已完成的修改、验证结果、当前状态、阻塞项和下一步。
- 有实质进展、状态变化或验证结果时追加日志；不要重复粘贴完整代码或冗长过程。

## 实施计划

- 实施计划存放在 `docs/execution-plans/`，文件名使用能说明任务的名称，建议以 `.md` 结尾。
- 计划至少说明目标、背景、范围、实施步骤、涉及文件、验证方式、风险与回滚方式，以及完成标准。
- 执行过程中及时更新计划状态和步骤进度；重要进展同时记录到当天的开发日志。
- 新建计划时可参考 `docs/execution-plans/TEMPLATE.md`。

## 工作约定

- 开始工作前先查看相关实施计划和最近的开发日志，恢复当前上下文。
- 涉及 `commit`、`push`、`tag` 或 PR 前，先加载 `docs/git-conventions.md` 并按其执行。
- 结束工作前确认文档与代码状态一致，并补充必要的开发日志。
