# 开发与构建

本文档记录 TomatoDesk 的开发环境、常用命令、安装包构建、CI 与发布流程。项目背景与功能介绍见 `README.md`。

## 环境要求

- Node.js 与 pnpm（`package.json` 的 `packageManager` 指定 pnpm 11.3.0）。
- Rust stable MSVC 工具链。
- Windows 开发还需要 WebView2 和 Visual Studio C++ Build Tools。

## 常用命令

```bash
pnpm install
pnpm dev
pnpm build
pnpm lint
pnpm check:design
pnpm tauri dev
```

- `pnpm check:design`：扫描 `src/`，检查旧色板 token、硬编码白色文字、彩色填充底，以及 `backdrop-blur` 是否只出现在浮层组件。规范见 `docs/design/design-language.md`。
- 开发模式下可用 URL 参数单独预览界面，不需要启动 Tauri 窗口：
  - `?styleguide`：设计基座（色板、排版、组件）。
  - `?preview&view=inbox|repos|search|settings`：用 `src/features/dev/fixtures.ts` 的假数据渲染页面，`&repo=<id>` 直接进仓库详情，`&thread=<id>` 直接开该仓库下的 PR / Issue 详情页。
  - `?capture`：快速收集窗口。
  - `?preview` 与 fixtures 只在 `import.meta.env.DEV` 下生效，生产构建会被摇掉。

工程进度和验证记录见 `docs/execution-plans/` 与 `docs/development-logs/`。

## 构建 Windows 安装包

在 Windows PowerShell 中运行：

```powershell
./scripts/build-installer.ps1
```

脚本使用 Windows MSVC Cargo，并将安装包、签名文件（`*.exe.sig`）、`latest.json`、SHA-256 校验文件和构建清单写入 `artifacts/installers/<版本>/<短提交号>/`。`artifacts/` 为本地构建产物，不纳入版本控制。

打包需要 updater 签名密钥：优先读取环境变量 `TAURI_SIGNING_PRIVATE_KEY`，未设置时回退到 `~/.tauri/tomatodesk.key`；两者都不存在时脚本直接失败，避免产出无法自动更新的安装包。

## CI 与 GitHub Release

GitHub Actions 在针对 `main` 的 Pull Request 和推送时运行 Windows 检查：前端 lint/build、Rust 格式检查、测试、check 和 Clippy。

发布版本时，先确保 `package.json` 与 `src-tauri/tauri.conf.json` 的版本一致，再推送匹配的版本标签：

```powershell
git tag v0.1.0
git push origin v0.1.0
```

Release workflow 会再次运行检查，验证标签与应用版本一致，在 Windows runner 上构建 NSIS 安装包，并发布安装程序、SHA-256 校验文件、签名文件和 `latest.json`。Release 正文取自 `docs/releases/<标签>.md`。当前安装包未进行代码签名。

## 自动更新与签名

客户端启动后延迟 8 秒从 GitHub Release 检查更新，也可通过托盘菜单「检查更新」手动触发。发现新版本后由用户确认下载，下载完成后再由用户确认重启；安装程序以 NSIS `passive` 模式运行，结束后自动重新启动应用。

### 更新源

`src-tauri/tauri.conf.json` 的 `plugins.updater.endpoints` 指向：

```text
https://github.com/AbelTomato/TomatoDesk/releases/latest/download/latest.json
```

每个 Release 需要额外上传 `latest.json`，由 `scripts/generate-latest-json.ps1` 生成。Windows NSIS 直接复用安装包本身：清单中的 `url` 指向 `*-setup.exe`，`signature` 是同名 `.exe.sig` 文件的完整内容（不是路径）。`build-installer.ps1` 在本地打包后会调用该脚本。

### 签名密钥

| 内容     | 位置                                                                          | 是否入库 |
| -------- | ----------------------------------------------------------------------------- | -------- |
| 公钥     | `src-tauri/tauri.conf.json` 的 `plugins.updater.pubkey`                       | 是       |
| 私钥     | 本地 `~/.tauri/tomatodesk.key`、GitHub Secrets `TAURI_SIGNING_PRIVATE_KEY`    | 否       |
| 私钥密码 | GitHub Secrets `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`（当前密钥无密码，可留空） | 否       |

生成密钥：

```powershell
pnpm tauri signer generate -w "$HOME\.tauri\tomatodesk.key"
```

更换密钥前必须先发布一个“用旧钥签名、内嵌新公钥”的过渡版本，否则已装用户无法继续自动更新。私钥与密码需要离线备份，丢失后只能让用户手动重装。

### 发布前检查

- `package.json` 与 `src-tauri/tauri.conf.json` 版本一致，标签为 `v<版本>`。
- `docs/releases/<标签>.md` 存在，会同时作为 Release 正文和 `latest.json` 的更新说明。
- Release workflow 会校验 `latest.json` 的版本、`url` 指向与 `.sig` 内容一致，不一致直接失败。
- 首个带 updater 的版本（0.2.0）需要 0.1.0 用户手动安装一次，之后才能进入自动更新循环。

### 已知限制

- 安装包未做代码签名，SmartScreen 仍会提示未知发布者；updater 签名只保证更新包的完整性与来源。
- 只有单通道，没有灰度和回滚；`latest.json` 一经发布即对全部用户生效。
- 不支持降级，数据库迁移需保持“只增不删”。

## 从 WSL 调用 Windows Cargo

WSL 可以通过 Windows PowerShell 使用主机上的 MSVC Rust 工具链。项目源码通过 WSL UNC 路径传给 PowerShell，编译产物放在 Windows 本地目录：

```bash
project_win_path="$(wslpath -w "$PWD")"
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass \
  -File "${project_win_path}\\scripts\\windows-cargo.ps1" check
```

脚本默认使用 `%TEMP%\TomatoDesk\cargo-target` 作为 `CARGO_TARGET_DIR`，也支持 `build`、`clippy` 和 `test` 命令以及额外 Cargo 参数，例如 `... build --release`。这样可以避免 Windows Cargo 在 `\\wsl.localhost` 文件系统中创建增量编译锁文件失败。

## 推荐的 IDE 配置

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
