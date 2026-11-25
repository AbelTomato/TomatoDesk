# TomatoDesk

TomatoDesk 是我个人在看到了一些文章之后，经过了思考，然后决定要给我自己搭建的一个个人工作站

本来我是在持续做TomatoAgent的Windows桌面端，然后做着做着感觉不太对劲，我尝试做Tracing，做Eval，跟风做这些东西，但是我实际上并不知道它效果好不好，我自己甚至都不会去用，而且我也不清楚具体要做什么方向，一个个人助手，这个范围太大了，要兼顾太多东西

所以我决定从产品的角度出发，做我自己真正会用的东西，这样才能有不断优化、迭代、更新的动机，于是TomatoDesk诞生了

## 技术栈

Tauri 2 · React · TypeScript · Vite · Tailwind CSS · Rust

## 主要功能

- **快速收集**：全局快捷键唤起轻量收集窗口，自动读取剪贴板中的链接或文本，可补充备注；网页元数据抓取失败时保留原始内容。
- **收件箱**：集中管理收集到的链接与笔记，支持待处理、已处理、归档状态和标签。
- **搜索**：基于 SQLite FTS5 的关键词检索，支持类型与状态筛选，`Ctrl+K` 直接聚焦搜索。
- **GitHub 更新**：添加仓库后同步 Release、Issue 和 PR，按来源事件去重，展示最近同步时间与错误信息。
- **本地优先**：内容、标签和凭据都留在本机，GitHub Token 存入 Windows Credential Manager。
- **桌面体验**：常驻系统托盘，同步完成或失败时发送 Windows 通知，支持 JSON / Markdown 导出。

## 平台与状态

当前面向 Windows x64，提供 NSIS 安装包，未做代码签名；macOS 与 Linux 尚未支持。

## 文档

- 开发环境、构建与发布流程：`docs/development.md`
- 实施计划：`docs/execution-plans/`
- 开发日志：`docs/development-logs/`
- 版本发布说明：`docs/releases/`
