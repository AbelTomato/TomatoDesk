# 实施计划：应用内自动更新与签名

- 状态：实施完成，待真机升级验证
- 创建日期：2026-11-25
- 最后更新：2026-11-25
- 负责人：Codex

## 目标

接入 Tauri updater，使已安装的 TomatoDesk 能从 GitHub Release 检查并安装新版本；更新包使用自有 minisign 密钥签名，客户端内嵌公钥校验后才允许安装。

## 背景与现状

- `v0.1.0` 已发布，安装包为未签名的 NSIS currentUser 安装程序，用户升级只能手动下载。
- 发布链路已具备 `build-installer.ps1`、SHA-256、`manifest.json` 和标签驱动的 Release workflow，但缺少 updater 清单与签名产物。
- 仓库公开，`releases/latest/download/latest.json` 无需鉴权即可访问，不需要额外后端或对象存储。
- 已有 `tauri-plugin-dialog`、`notification`，主窗口与托盘结构完整，更新提示可复用现有布局。

## 设计决策

- 更新源：单一 GitHub Release，endpoint 固定为 `https://github.com/AbelTomato/TomatoDesk/releases/latest/download/latest.json`，不预留 beta 通道（当前单用户）。
- 交互：启动后延迟 8 秒自动检查；发现新版本后由用户确认下载，下载完成后由用户确认重启，不自动重启。
- 签名：只做 updater 的 minisign 签名，暂不采购 Authenticode 代码签名证书；SmartScreen 提示仍然存在。
- 密钥：私钥放在 `~/.tauri/tomatodesk.key` 与 GitHub Secrets，公钥写入 `tauri.conf.json`；当前生成为无密码密钥，0.2.0 发布前可零成本更换为带密码密钥。
- 不使用 `tauri-plugin-process`：Windows 上 `install()` 会退出应用并由安装器重启，`relaunch()` 不可达。

## 范围

### 包含

- 接入 `tauri-plugin-updater`，配置 endpoints、公钥与 NSIS `passive` 安装模式。
- 打开 `createUpdaterArtifacts`，构建产出安装包签名文件 `*.exe.sig`（v2 的 NSIS 直接复用安装包，没有独立归档）。
- 前端更新提示条：自动检查、手动检查、下载进度、确认重启、跳过版本、错误提示。
- 托盘菜单新增“检查更新”，通过事件触发主窗口检查。
- 发布链路生成 `latest.json` 并在发布前校验版本、URL 与签名一致。
- 更新 `docs/development.md`、实施计划与开发日志。

### 不包含

- 代码签名证书与 SmartScreen 消除。
- beta 通道、灰度发布、更新回滚。
- macOS / Linux 更新支持。
- 0.1.0 → 0.2.0 的真机升级验收（需在发布 0.2.0 前单独执行）。

## 实施步骤

1. [x] 生成 minisign 密钥对，公钥写入 `src-tauri/tauri.conf.json`，`.gitignore` 忽略 `*.key`。
2. [x] 增加 `tauri-plugin-updater` 依赖、`updater:default` 权限并在 `lib.rs` 注册插件。
3. [x] 新增 `src/lib/update.ts` 与 `src/features/update/UpdateBanner.tsx`，在 Inbox 顶部挂载。
4. [x] 托盘菜单新增“检查更新”，点击后显示主窗口并发送 `check-updates-requested`。
5. [x] 新增 `scripts/generate-latest-json.ps1`，改造 `build-installer.ps1` 收集 updater 产物并强制要求签名密钥。
6. [x] `release.yml` 注入签名密钥并在发布前校验 updater 归档、`.sig` 与 `latest.json`。
7. [x] 本地 Rust 校验与带签名的完整打包验证（干净 target 完成，见进度记录）。
8. [ ] 0.1.0 → 0.2.0 真机升级与数据保留验收。

## 涉及文件与模块

- `src-tauri/tauri.conf.json`：updater endpoints、公钥、`createUpdaterArtifacts`。
- `src-tauri/Cargo.toml`、`src-tauri/src/lib.rs`、`src-tauri/capabilities/default.json`：插件与权限。
- `src-tauri/src/platform/mod.rs`：托盘“检查更新”菜单与事件。
- `src/lib/update.ts`、`src/features/update/UpdateBanner.tsx`、`src/features/inbox/Inbox.tsx`：前端更新流程。
- `scripts/generate-latest-json.ps1`、`scripts/build-installer.ps1`、`.github/workflows/release.yml`：发布链路。
- `.gitignore`、`docs/development.md`：密钥忽略规则与操作说明。

## 验证方式

- 已完成：`pnpm install`、`pnpm lint`、`pnpm build` 通过；三个 PowerShell 脚本语法解析通过；`tauri.conf.json`、`capabilities/default.json` 可解析且字段正确；`release.yml` YAML 解析通过，步骤顺序正确。
- 已完成：Windows Cargo `fmt --check`、`clippy --locked --all-targets`、`test --locked`（9/9）通过，使用干净 target `%TEMP%\TomatoDesk\cargo-target-check`。
- 已完成：`build-installer.ps1` 完整执行成功，产出 `TomatoDesk_0.1.0_x64-setup.exe`（3.58 MiB）、`.exe.sig`、`.exe.sha256`、`manifest.json` 与 `latest.json`；`latest.json` 的 `url` 指向安装包、`signature` 与 `.exe.sig` 内容一致。
- 待完成：0.1.0 安装后升级到 0.2.0 且数据库保留。
- 待完成：篡改归档后验签失败、断网与 `latest.json` 404 静默失败、跳过版本不再提示。

## 风险与回滚

- 风险：私钥丢失将导致已装用户无法自动更新，必须离线备份私钥。
- 风险：`latest.json` 的 `url` 指向 exe 或签名内容不匹配会导致更新全部失败；已在发布前加校验。
- 风险：安装包仍未签名，用户首次安装会看到 SmartScreen 提示。
- 风险：0.1.0 无 updater，老用户必须手动安装一次 0.2.0 才能进入自动更新循环。
- 回滚：删除 `plugins.updater` 与 `createUpdaterArtifacts`、移除前端入口即可退回手动发布；已发布的标签不移动，必要时发补丁版本。

## 完成标准

- [x] 客户端能读取 `latest.json` 并按用户确认下载、安装、重启。
- [x] 发布链路强制签名，并在发布前校验签名与清单一致。
- [x] 本地与 CI 的 Rust 检查通过。
- [ ] 一次真实的 0.1.0 → 0.2.0 升级验证数据完整。

## 进度记录

- 2026-11-25 09:51：生成 `~/.tauri/tomatodesk.key`（无密码）与公钥，公钥写入 `tauri.conf.json`；确认仓库为 public，`releases/latest` 可直接访问。
- 2026-11-25 09:55：完成插件接入、前端更新条、托盘入口、`latest.json` 生成脚本与 release workflow 改造。`pnpm lint`、`pnpm build` 通过；脚本与配置语法校验通过。
- 2026-11-25 09:55：本地 `windows-cargo.ps1 check` 因共享 target 缺少 `libsqlite3-sys` 的 `bindgen.rs` 失败（2026-11-24 已记录的既有问题），命令随后被沙箱策略拦截，未继续重试；Rust 校验改由干净 target 或 CI 完成。
- 2026-11-25 10:12：改用 `%TEMP%\TomatoDesk\cargo-target-check` 干净 target 重跑，`cargo fmt --check`、`clippy --locked --all-targets` 通过（仅保留既有测试建议），`cargo test --locked` 9/9 通过。
- 2026-11-25 10:36：清掉共享 target 中损坏的 `libsqlite3-sys` 构建缓存后完成 release 打包。实测 v2 的 NSIS 只产出 `setup.exe` 与 `setup.exe.sig`，没有 `.nsis.zip`，据此把 `build-installer.ps1`、`generate-latest-json.ps1` 与 release 校验改为指向安装包本身；重跑成功，`latest.json` 的 `url`、`signature` 与产物一致。
