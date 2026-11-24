# 实施计划：TomatoDesk MVP

- 状态：进行中（步骤 A、B、C 已完成，下一步为步骤 D）
- 创建日期：2026-11-23
- 最后更新：2026-11-24
- 负责人：项目负责人

## 目标

构建一个 Windows 优先、本地优先的个人信息收件箱：通过全局快捷键快速收集链接或想法，在 Inbox 中搜索、处理和归档，并查看 GitHub Release、Issue、PR 更新。

第一版的判断标准是能否连续使用，并减少翻收藏夹和 GitHub 的时间。

## 背景与现状

当前仓库没有既有应用代码，技术方案已经确定为 Tauri 2 + React + TypeScript + Vite + Tailwind CSS。执行范围采用快速验证版，代码边界采用可持续本地产品版。

移动端只要求前端布局和领域模型具备复用条件。全局快捷键、托盘和 Windows 通知放在平台适配层，移动端入口以后通过系统分享能力补充。

## 范围

### 包含

- Tauri 2 Windows 桌面应用
- React、TypeScript、Vite 和 Tailwind CSS
- 全局快捷键唤起快速收集窗口
- 从剪贴板读取 URL 或普通文本
- URL 标题、描述和规范链接抓取
- SQLite、FTS5、Inbox、状态、标签和搜索
- GitHub 仓库配置
- Release、Issue、PR 同步、去重和错误状态
- 手动同步和应用运行期间的定时同步
- Windows Credential Manager 保存 GitHub Token
- JSON/Markdown 导出
- Windows NSIS 安装包

### 不包含

- Tomato-Agent
- 邮件接入
- 微信和 QQ 接入
- 浏览器扩展
- 账号体系和云端同步
- 自动回复、自动改代码等高风险动作
- Commit 流和完整 GitHub 动态流
- 自动更新

## 技术决策

| 层级     | 选择                       | 约束                             |
| -------- | -------------------------- | -------------------------------- |
| 桌面容器 | Tauri 2                    | Windows 优先，保留移动端可能性   |
| 前端     | React + TypeScript + Vite  | 页面逻辑不依赖 Windows API       |
| 样式     | Tailwind CSS               | 桌面双栏，窄屏单栏               |
| 后端     | Rust                       | 负责业务、数据库、网络和密钥     |
| 数据库   | SQLite + FTS5              | 数据保存在应用 `userData` 目录   |
| GitHub   | REST API                   | Fine-grained PAT，按最小权限配置 |
| 密钥     | Windows Credential Manager | Token 不写入数据库和日志         |
| 打包     | Tauri NSIS                 | 首版不做自动更新                 |

## 目标目录结构

```text
src/
  app/
  components/
  features/
    capture/
    inbox/
    github/
    search/
  lib/
  styles/

src-tauri/
  migrations/
  src/
    commands/
    domain/
    application/
    infrastructure/
      db/
      github/
      secrets/
    platform/
```

## 实施步骤

每个步骤完成后先保持应用可启动，再进入依赖它的步骤。

### A. 工程骨架

1. [x] 确认 Windows 环境中的 Node/pnpm、WebView2、Visual Studio C++ Build Tools 和 Rust stable/Cargo MSVC 工具链。
2. [x] 创建 Tauri 2 + React + TypeScript + Vite 项目。
3. [x] 配置 pnpm、TypeScript、ESLint 和格式化规则。
4. [x] 配置 Tailwind CSS、颜色、间距和断点约定。
5. [x] 建立前端 `features` 目录和 Rust 分层目录。
6. [x] 配置开发环境、生产环境和应用名称。
7. [x] 确认 Windows 开发模式可启动和关闭。

完成条件：空应用可启动，前端和 Rust Command 均能完成一次调用。

### B. 应用窗口和平台能力

1. [x] 配置主窗口。
2. [x] 配置快速收集窗口。
3. [x] 接入全局快捷键插件。
4. [x] 实现快捷键打开或聚焦快速收集窗口。
5. [x] 接入系统托盘。
6. [x] 添加显示主窗口、快速收集、退出应用菜单项。
7. [x] 接入 Windows 通知。
8. [x] 将快捷键、托盘和通知封装在 `platform` 适配层。

完成条件：应用可驻留托盘，快捷键能打开快速收集窗口，关闭主窗口不会退出应用。

### C. 数据库和领域模型

1. [x] 选择 SQLite 文件路径并自动创建目录。
2. [x] 建立迁移执行器。
3. [x] 创建 `items` 表。
4. [x] 创建 `tags` 和 `item_tags` 表。
5. [x] 创建 `github_repos` 表。
6. [x] 创建 `github_events` 表。
7. [x] 创建 `sync_runs` 表。
8. [x] 创建 FTS5 虚拟表和同步逻辑。
9. [x] 定义 `ItemKind`、`ItemStatus`、`GithubEventType` 等领域枚举。
10. [x] 为来源事件建立唯一约束，保证同步幂等。
11. [x] 实现 Item、Tag、GithubRepo、SyncRun Repository。
12. [x] 定义统一数据库错误类型。

核心字段：

```text
items: id, kind, source, url, title, summary, note, status,
       created_at, updated_at
github_repos: id, owner, name, enabled, last_synced_at, etag
github_events: id, repo_id, source_event_id, event_type,
               payload_json, occurred_at
sync_runs: id, source, started_at, finished_at, status, error
```

完成条件：应用启动时能执行迁移；能插入、读取、更新和删除 Item；搜索索引能返回结果。

### D. IPC 和应用服务

1. [ ] 统一 Command 错误响应格式。
2. [ ] 定义前端与 Rust 共用的 DTO。
3. [ ] 为 Command 输入增加结构校验。
4. [ ] 实现 `create_item`、`get_item`、`list_items`、`update_item`。
5. [ ] 实现 `archive_item` 和 `search_items`。
6. [ ] 实现 `list_tags`、`create_tag`、`set_item_tags`。
7. [ ] 为前端封装统一 `api` 调用层。
8. [ ] 禁止组件直接调用原始 `invoke`。

完成条件：前端可以通过统一 API 完成 Item 创建、列表、搜索、更新和归档。

### E. 快速收集

1. [ ] 读取当前剪贴板文本。
2. [ ] 判断内容是 URL 还是普通文本。
3. [ ] 设计快速收集窗口的最小输入状态。
4. [ ] 支持修改 URL 和添加备注。
5. [ ] 保存普通文本为 `note`。
6. [ ] 保存 URL 为 `link`。
7. [ ] 保存成功后关闭窗口并聚焦 Inbox。
8. [ ] 使用 Rust 请求网页元数据。
9. [ ] 提取 `title`、`description`、`og:title`、`og:description`、`canonical`。
10. [ ] 设置元数据请求超时和失败降级。
11. [ ] 元数据补充失败时保留原始 URL 和备注。

完成条件：从任意 Windows 应用复制链接后按快捷键即可保存，网络失败不会丢失内容。

### F. Inbox 和搜索界面

1. [ ] 建立应用级布局和导航。
2. [ ] 实现 Inbox 列表和 Item 详情面板。
3. [ ] 实现待处理、已处理、归档状态切换。
4. [ ] 实现标签展示和编辑。
5. [ ] 实现关键词搜索、类型筛选和状态筛选。
6. [ ] 实现最近更新、最近创建排序。
7. [ ] 实现打开原链接。
8. [ ] 实现 `Ctrl+K` 聚焦搜索。
9. [ ] 实现窄屏单栏布局。
10. [ ] 处理加载、空状态、失败和无结果状态。

完成条件：用户可以找到旧内容，并完成搜索、查看、加标签、处理和归档。

### G. GitHub 同步

1. [ ] 设计添加仓库表单。
2. [ ] 解析并校验 GitHub 仓库 URL。
3. [ ] 实现 PAT 保存、读取和删除。
4. [ ] 配置 GitHub API 客户端和请求超时。
5. [ ] 实现 Release、Issue 拉取。
6. [ ] 从 Issue 响应中识别 Pull Request。
7. [ ] 将 GitHub 响应转换为统一 Item。
8. [ ] 使用 `source_event_id` 去重。
9. [ ] 保存请求 ETag 和最后同步时间。
10. [ ] 实现手动同步。
11. [ ] 实现应用运行期间的定时同步。
12. [ ] 记录同步开始、成功、失败状态。
13. [ ] 展示最近同步时间和错误信息。
14. [ ] 对 GitHub 限流和网络失败做可读提示。

完成条件：添加仓库后能获得 Release、Issue、PR 更新；重复同步不会产生重复数据；失败可重试。

### H. 导出、通知和安装包

1. [ ] 实现 JSON 导出。
2. [ ] 实现 Markdown 导出。
3. [ ] 在托盘菜单中加入快速收集和主窗口入口。
4. [ ] 增加同步完成和同步失败通知。
5. [ ] 设置 Windows 应用图标和安装信息。
6. [ ] 配置 NSIS 安装包。
7. [ ] 验证全新安装、升级安装和卸载。
8. [ ] 验证卸载不会误删用户数据。

完成条件：没有开发环境的 Windows 机器也能安装并完成收集、搜索和 GitHub 同步。

### I. 最终验收

1. [ ] 从浏览器复制 URL 并完成快速收集。
2. [ ] 从终端复制普通文本并保存为笔记。
3. [ ] 重启应用并确认数据完整。
4. [ ] 搜索标题、备注和 GitHub 内容。
5. [ ] 添加多个仓库并重复同步。
6. [ ] 断网后收集内容并在恢复网络后继续同步。
7. [ ] 确认 Token 不出现在数据库、界面和日志。
8. [ ] 验证窄窗口布局。
9. [ ] 导出数据并检查内容完整。
10. [ ] 记录真实使用中的阻塞点和下一阶段需求。

## 涉及文件与模块

- `package.json`：前端脚本、依赖和构建命令。
- `scripts/windows-cargo.ps1`：从 WSL PowerShell 调用 Windows MSVC Cargo，并配置本地 target 目录。
- `.prettierignore`：排除 Tauri 自动生成的 schema 文件。
- `src/`：React 页面、组件、功能模块和 Tauri API 封装。
- `src-tauri/Cargo.toml`：Rust 依赖和插件。
- `src-tauri/src/commands/mod.rs`：导出 Tauri command 及其宏生成项。
- `src-tauri/src/commands/`：前端调用的 Command。
- `src-tauri/src/domain/`：实体、枚举和领域错误。
- `src-tauri/src/application/`：收集、搜索、同步和导出服务。
- `src-tauri/src/infrastructure/db/`：SQLite 连接、迁移和 Repository。
- `src-tauri/src/infrastructure/github/`：GitHub API 客户端和响应转换。
- `src-tauri/src/infrastructure/secrets/`：Credential Manager 适配。
- `src-tauri/src/platform/`：快捷键、托盘和通知适配。
- `src-tauri/migrations/`：数据库迁移文件。
- `src-tauri/tauri.conf.json`：窗口、权限和打包配置。

## 验证方式

- 使用 `pnpm tauri dev` 验证开发模式启动。
- 在 WSL 中通过 `scripts/windows-cargo.ps1 check` 调用 Windows MSVC Cargo，验证 Rust 后端编译。
- 使用 Rust 格式化和编译命令确认后端构建无错误。
- 在 Windows 上人工验证快捷键、托盘、剪贴板和通知。
- 使用多个链接和笔记验证保存、搜索、状态和导出。
- 使用至少两个 GitHub 仓库验证首次同步、增量同步、重复同步和失败重试。
- 断网后验证本地收集仍可用，恢复网络后同步可继续。
- 使用生产构建安装包验证全新安装、升级安装、卸载和数据保留。

## 进度记录

- 2026-11-23：完成步骤 A 的工程初始化、目录分层、Tailwind 主题、环境配置和 `get_app_info` Command；`pnpm build`、`pnpm lint`、`pnpm format:check`、Rust 格式检查通过。当前在 Linux 环境，Windows 依赖和 Tauri 启动待 Windows 验证；`cargo check` 因 crates.io 下载超时未完成，详见 `docs/development-logs/2026-11-23.md` 17:07:41 记录。
- 2026-11-23：尝试 `CARGO_NET_OFFLINE=true pnpm tauri dev`，Vite 在 `http://localhost:1420/` 成功启动，但 Rust 因本地未缓存 `tauri-plugin-opener` 依赖退出；详见 `docs/development-logs/2026-11-23.md` 17:16:01 记录。
- 2026-11-23：从 WSL PowerShell 调用 Windows `cargo 1.99.1`（`stable-x86_64-pc-windows-msvc`），使用 Windows 本地 `CARGO_TARGET_DIR` 后 `cargo check` 通过；修正 `commands` 模块对 Tauri command 宏生成项的导出，并新增 `scripts/windows-cargo.ps1`。WebView2、C++ Build Tools 和 Tauri 窗口启动仍待验证。
- 2026-11-23：按 README 的 WSL 调用方式重新执行 `scripts/windows-cargo.ps1 check --locked`，确认 Windows Cargo 使用 `C:\Users\ABELTO~1\AppData\Local\Temp\TomatoDesk\cargo-target` 并成功完成检查；剩余待验证项不变。
- 2026-11-23：在 Windows 本地临时工作副本中完成 `pnpm tauri info` 和 `pnpm tauri dev`；确认 WebView2/Visual Studio/MSVC/Node/pnpm 可用，窗口响应正常，按钮调用 Rust `get_app_info` 后显示 `TomatoDesk 0.1.0 · development`。修正 Vite Windows WebView 的 `/@vite/client` 兼容处理，步骤 A 完成，进入步骤 B。
- 2026-11-23：将 Vite 兼容处理限定为 Windows 后重新启动临时工作副本，页面和 Rust Command 验证仍通过；`pnpm build`、`pnpm lint`、`pnpm format:check`、Rust 格式检查和 Windows Cargo `check --locked` 均通过。
- 2026-11-24：完成步骤 C。SQLite 位于 Tauri `app_data_dir`，自动建目录并执行版本化迁移；建立领域模型、Item/Tag/GitHub repo/GitHub event/SyncRun Repository、FTS5 触发器及事件唯一约束。5 项 Rust 数据库测试通过；Windows Cargo `check --locked`、Clippy、Rust 格式、`pnpm build` 和 `pnpm lint` 通过。Tauri 开发应用启动后在 `%APPDATA%\com.tomatodesk.app\tomatodesk.sqlite3` 创建数据库，SQLite 查询确认迁移版本 1 和所需表。Clippy 保留步骤 B 的 `show_notification` 未使用警告；应用已关闭。
