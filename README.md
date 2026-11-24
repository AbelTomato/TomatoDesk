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
