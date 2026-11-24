# TomatoDesk

TomatoDesk 是一个 Windows 优先、本地优先的个人信息收件箱，使用 Tauri 2、React、TypeScript、Vite、Tailwind CSS 和 Rust 构建。

## 开发

```bash
pnpm install
pnpm dev
pnpm build
pnpm lint
pnpm tauri dev
```

Windows 开发还需要 WebView2 和 Visual Studio C++ Build Tools。工程进度和验证记录见 `docs/execution-plans/` 与 `docs/development-logs/`。

### 构建 Windows 安装包

在 Windows PowerShell 中运行：

```powershell
./scripts/build-installer.ps1
```

脚本使用 Windows MSVC Cargo，并将安装包、SHA-256 校验文件和构建清单写入 `artifacts/installers/<版本>/<短提交号>/`。`artifacts/` 为本地构建产物，不纳入版本控制。

## CI 与 GitHub Release

GitHub Actions 在针对 `main` 的 Pull Request 和推送时运行 Windows 检查：前端 lint/build、Rust 格式检查、测试、check 和 Clippy。

发布版本时，先确保 `package.json` 与 `src-tauri/tauri.conf.json` 的版本一致，再推送匹配的版本标签：

```powershell
git tag v0.1.0
git push origin v0.1.0
```

Release workflow 会再次运行检查，验证标签与应用版本一致，在 Windows runner 上构建 NSIS 安装包，并发布安装程序、SHA-256 校验文件和 manifest。当前安装包未进行代码签名。

### 从 WSL 调用 Windows Cargo

WSL 可以通过 Windows PowerShell 使用主机上的 MSVC Rust 工具链。项目源码通过 WSL UNC 路径传给 PowerShell，编译产物放在 Windows 本地目录：

```bash
project_win_path="$(wslpath -w "$PWD")"
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass \
  -File "${project_win_path}\\scripts\\windows-cargo.ps1" check
```

脚本默认使用 `%TEMP%\TomatoDesk\cargo-target` 作为 `CARGO_TARGET_DIR`，也支持 `build`、`clippy` 和 `test` 命令以及额外 Cargo 参数，例如 `... build --release`。这样可以避免 Windows Cargo 在 `\\wsl.localhost` 文件系统中创建增量编译锁文件失败。

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
