# 收件箱邮件连接器（阶段五）接入方案

- 状态：S0 完成（163 真实账号已跑通登录 + `ID` + `EXAMINE` + 取信），S1 完成（契约与存储层），S2 完成（解析层与 fixtures），S3 完成（IMAP 取信与多账户轮询），S4 完成（命令层与 DTO 校验），S5 待开始
- 创建日期：2026-11-26
- 最后更新：2026-11-27
- 涉及模块：`inbox` / `inbox_sources` / `inbox_messages` / `secrets` / `settings` / `inbox-ui`
- 目标版本：v0.5.0
- 前置文档：`docs/execution-plans/2026-11-26-capture-inbox-split.md`（第 8 节阶段五）

## 0. 评审结论（2026-11-26）

| 议题 | 结论 |
| --- | --- |
| 账户数量 | **多账户**，上限 5；单账户是「上限 = 1」的配置特例，不写两套代码 |
| 已读回写 | 不回写服务器 `\Seen`，本地已读只代表看过 |
| 首次窗口 | 最近 14 天、单轮每账户 100 封 |
| 同步范围 | 只取 `INBOX`，不订阅其他文件夹 |
| 服务商范围 | 仅支持授权码 / 应用专用密码的 IMAP；不支持 Outlook / M365 与 NTLM |
| 删除账户 | 软停用：删凭据、清配置、留消息 |

## 1. 目标与结论摘要

在不改三个领域边界的前提下，把邮箱接成收件箱的第二个消息源：邮件只作为**待判断的事件**进入 `inbox_messages`，由人决定忽略或转为待办，不复制邮件客户端能力。

先给结论，后面逐项展开：

1. **不需要新迁移。** `inbox_sources.kind` 已允许 `email`，`config_json` / `cursor` 已在表里，`inbox_messages` 的字段足够承载邮件；一个账户一行来源，天然支持多账户。数据库版本停在 v8。
2. **接入点是 `application/inbox/email.rs` + 新增 `infrastructure/email/`**，与 `github.rs` 同层同构。判断动作、去重、分页、角标、事件全部复用，不改现有 SQL。
3. **多账户，单账户是上限为 1 的特例。** 来源 id 由「地址 + 服务器」确定性派生，凭据按来源 id 分开存，轮询按账户串行、失败隔离。
4. **只读收信。** 授权码登录，`EXAMINE` + `BODY.PEEK[]`；不写回已读、不下载附件、不发信、不做会话聚合、不做 OAuth2。
5. **凭据只进 Windows 凭据管理器**，`config_json` 只放主机、端口、加密方式、邮箱地址、文件夹等非敏感配置；密码不出现在 IPC 返回值、日志和 `payload_json` 里。
6. **服务商差异用能力探测消化**，不按域名写分支：登录后读 `CAPABILITY`，按需发 `ID`、回退 `AUTHENTICATE PLAIN`、处理 UTF-7 文件夹名。真正不支持的只有 OAuth2-only 与 NTLM。

## 2. 现有接入面盘点

阶段一至四已经把收件箱做成「来源 + 消息 + 判断」的通用骨架，邮件要接的地方比想象中少。逐项核对如下。

| 能力 | 现状 | 邮件能否复用 | 需要补什么 |
| --- | --- | --- | --- |
| 消息源注册 | `inbox_sources(kind, name, config_json, cursor, enabled, last_error)`，一源一行 | 复用，且天然多账户 | 仓储补 `upsert_email`、`config`、`save_config`、`set_enabled`、`clear_cursor`、`list_email` |
| 入库去重 | `INSERT ... ON CONFLICT(source_id, external_id)` | 完全复用 | 无（只要求 `external_id` 稳定） |
| 已读 / 忽略 / 转待办 | `application::inbox` + 事务 | 完全复用 | 无 |
| 分页 / 筛选 / 计数 | `InboxMessageRepository` | 完全复用 | 无 |
| 同步事件 | `inbox-sync-updated`（载荷 `Option<InboxSyncSummary>`，含 `sourceId`） | 复用 | 每个账户同步完各发一次 |
| 凭据存储 | `infrastructure/secrets`，键名 `github-token` | 复用机制 | 加 `email-password:<source_id>`，一账户一键 |
| 已读回写队列 | `inbox_read_back`，worker 只调 GitHub API | 不复用 | 本阶段明确不做；`kind` 过滤已挡住 |
| 待办来源展示 | `origin_source_kind` 子查询 + `describeOrigin()` | 完全复用 | 已支持「来自邮件」 |
| 收件箱来源栏 | 遍历 `list_inbox_sources`，可筛选任意来源 | 完全复用，多账户自动出现 | 无 |
| 收件箱详情 | 「相关」区写死 GitHub 文案 | 需条件化 | 按来源类型分支 |
| 设置页 | GitHub Token / 已读回写 / 仓库 / 导出 | 复用版式 | 新增「邮箱账户」列表与表单 |

三个容易踩的既有约束，先记下来：

- `inbox_messages.source_id REFERENCES inbox_sources(id) ON DELETE CASCADE`：**删除来源会连带删除该来源的全部消息**。所以删除账户只能 `enabled = 0` + 清配置，不能删 `inbox_sources` 行。
- `InboxMessageRepository::ingest` 只校验标题和分类非空，长度上限在命令层 DTO 里。连接器不走 DTO，必须自己按同一组常量截断（标题 512 / 摘要 4096 / 正文 65536 / 发件人 256 / 外部 id 512）。
- `Database::migrate` 只补齐「比库里多」的迁移，**不会拒绝更高的库版本**。跨阶段降级必须恢复迁移备份，不能指望旧二进制拒绝启动。

## 3. 接入位置与方式

### 3.1 三种接入方式的选择

| 方案 | 做法 | 评价 |
| --- | --- | --- |
| A. 独立 `email` 领域模块 | 自己的表、自己的判断流程 | 否决：判断、去重、待办转换会全部重写一遍，回到拆分前的老路 |
| B. `inbox` 下的 provider | 邮件只负责「拉取 + 映射 + 入库」 | **采用**：与 GitHub 完全同构，判断逻辑零改动 |
| C. 外部进程 / sidecar | 单独的可执行文件走 IPC | 否决：打包、签名、崩溃恢复成本远超收益 |

阶段五的定位就是 B：**连接器是收件箱的一个数据来源，不是第四个领域。**

### 3.2 分层落点

```text
┌─────────────────────────────────────────────────────────────┐
│ 前端                                                         │
│  SettingsPage（账户列表 + 新增/编辑/删除 + 测试连接）          │
│  InboxPage（来源栏自动列出多个邮箱、同步时间、来源提示）        │
│  InboxMessageDetail（按来源分支「相关」区）                    │
└───────────────┬─────────────────────────────────────────────┘
                │ IPC：list / save / test / delete_email_account
┌───────────────▼─────────────────────────────────────────────┐
│ commands/email.rs        入参校验、错误映射、触发首轮同步      │
├─────────────────────────────────────────────────────────────┤
│ application/inbox/email.rs   多账户轮询、增量、映射、游标推进   │
│ application/inbox/mod.rs     入库 / 判断（已有，不改）          │
├─────────────────────────────────────────────────────────────┤
│ infrastructure/email/    IMAP 会话、能力探测、MIME 解析         │
│ infrastructure/secrets   每账户授权码读写（keyring）           │
│ repositories/inbox_*     source 配置读写、消息 upsert（已有）   │
└─────────────────────────────────────────────────────────────┘
```

### 3.3 为什么解析放在 infrastructure

`infrastructure/github` 负责 HTTP 与 JSON，`application/inbox/github.rs` 负责业务映射，邮件按同样切法：

- `infrastructure/email`：连接、登录、能力探测、选箱、按 UID 取信、把原始字节交给 `mail-parser`，输出 `ParsedMail`（主题、发件人、日期、纯文本正文、附件名、Message-ID、UID）。这一层不碰数据库、不认识 `NewInboxMessage`。
- `application/inbox/email.rs`：遍历账户、读配置和游标、调客户端、把 `ParsedMail` 映射成 `NewInboxMessage`、写库、推进游标、发事件。

这样解析层可以只用 `.eml` fixtures 做纯函数测试，不需要数据库，也不需要网络。

### 3.4 账户身份与来源 id

多账户的关键不是 UI，而是「一个账户在数据库里是谁」。方案：

- 来源 id 由 **UUID v5 确定性派生**：`source-email-<uuid5(固定命名空间, "email|<小写地址>|<小写主机>")>`。
- 同一地址 + 同一服务器 → 永远同一个 id：重复保存是更新，不是新增；删除后重新添加会复用同一来源，历史消息还在。
- 改了地址或服务器 → 新 id，等于新增一个账户；旧账户需要手动删除（设置页在编辑时锁住这两个字段，避免误建）。
- `inbox_sources.name` 用邮箱地址，来源栏直接显示；同名不同服务器的账户在设置页用「地址 + 主机」区分。
- 上限 5 个：保存时统计 `kind = 'email' AND config_json IS NOT NULL` 的行数，超出返回 `validation`。

不需要额外的唯一索引：确定性 id 已经保证同一账户只有一行，`inbox_sources.id` 是主键。

### 3.5 多账户的实现复杂度评估

逐层看增量，结论是**多账户的边际成本很低，后补的成本很高**。

| 层 | 单账户做法 | 多账户增量 | 复杂度 |
| --- | --- | --- | --- |
| 数据库 | 固定 `source-email` | 一行一账户，无 schema 变更 | 无 |
| 账户标识 | 常量 id | UUID v5 派生 + 上限校验 | 低（约 20 行） |
| 凭据 | 固定 keyring 键 | 键名带 source id，多读一次循环 | 低 |
| 轮询 | 一个账户的循环 | 遍历启用账户、串行、每账户独立退避 | 中（约 80 行） |
| 命令与 DTO | 单对象 | `list` + `id` 入参，多一层列表 | 低 |
| 设置页 | 单表单 | 列表 + 新增/编辑/删除三个状态 | 中（约 120 行） |
| 收件箱页 | — | **零改动**，来源栏本来就遍历来源 | 无 |
| 测试 | 单账户用例 | 账户隔离、单账户失败不影响其他账户 | 低 |

判断：

- 多账户真正新增的只有「账户列表状态机」和「轮询里的账户循环」，其余都是把常量换成变量。
- 如果先做单账户，后续要改的是**来源 id 规则、凭据键名、IPC 形状和设置页结构**，属于破坏性返工，还会让已入库消息的归属变得含糊。
- 单账户需求可以用 `MAX_EMAIL_ACCOUNTS = 1` 表达，不需要另一套代码。

因此方案按多账户设计，上限 5。

### 3.6 不同服务商的实现复杂度

按「通用路径能否覆盖」分三档，而不是按域名写 if。

| 服务商 | 服务器（默认） | 认证 | 需要注意 | 支持级别 |
| --- | --- | --- | --- | --- |
| QQ 邮箱 | `imap.qq.com:993` | 授权码 + `LOGIN` | 连接数有限，多账户必须串行；不需要 `ID` | 完整支持 |
| 163 / 126 | `imap.163.com:993` | 授权码 + `LOGIN` | **必须发 `ID` 命令**，否则 `SELECT` 报 `Unsafe Login` | 完整支持（能力探测） |
| Gmail | `imap.gmail.com:993` | 应用专用密码（需先开两步验证） | 连接数限制；标签文件夹是 `[Gmail]/...`，只取 `INBOX` 可绕开 | 完整支持 |
| iCloud | `imap.mail.me.com:993` | App 专用密码 | 无 | 完整支持 |
| 自建 Dovecot / Postfix | 自定义 | `LOGIN` / `AUTHENTICATE PLAIN` | 自签证书会握手失败；部分只开 143 + STARTTLS | 有条件支持（需有效证书） |
| 企业 Exchange（本地） | 自定义 | 常见只开 NTLM | 本方案不实现 NTLM | 不支持 |
| Outlook / Microsoft 365 | — | OAuth2 / XOAUTH2 | 基本认证已关闭 | 不支持 |

三档划分：

- **Tier A｜通用路径**：QQ、Gmail、iCloud、证书有效的自建服务器。`LOGIN` + `EXAMINE INBOX` + `BODY.PEEK[]` 即可，**零服务商分支**。
- **Tier B｜一次实现、全服务商受益的兼容层**：`ID` 命令（163 必需）、`AUTHENTICATE PLAIN` 回退（部分服务器禁用 `LOGIN`）、UTF-7 文件夹名编解码（非 ASCII 文件夹）、`STARTTLS`、错误文案映射。这些都是按 `CAPABILITY` 判断，不写域名判断。
- **Tier C｜明确不支持**：OAuth2-only（Outlook/M365）、NTLM、自签证书。设置页与发布说明写清楚，避免用户反复试错。

复杂度的真实分布：**协议本身很统一，难点在认证策略与服务器策略**，不在解析。所以不按 provider 建适配器，只建一个能力探测层；新增一个服务商通常只需要加一条「预填主机/端口」和一条文档说明，不需要新代码路径。

## 4. 数据流

### 4.1 账户配置流（用户主动）

```text
设置页填表 ──▶ test_email_account ──▶ 连接/登录/ID/EXAMINE ──▶ 返回邮箱概况
                     │失败
                     ▼
              校验错误（不落库）

保存 ──▶ save_email_account
          ├─ 参数校验（地址、主机、端口、加密方式）
          ├─ 派生 source id，检查账户上限
          ├─ 先测一次连接，失败直接返回，不写半配置
          ├─ 写 config_json（address/host/port/tls/mailbox）
          ├─ keyring 写入 email-password:<source_id>
          ├─ upsert 来源行 + enabled=1 + 清空 last_error
          └─ spawn 一次该账户的立即同步（首屏不用等下一轮）
```

### 4.2 同步流（后台，无人值守）

```text
每 10 分钟 tick（首轮立即）
  ├─ 取所有 kind=email 且 enabled=1 且有配置的账户；为空则整轮跳过（不请求、不发事件）
  ├─ 逐账户串行（避免同一服务商并发连接被限流）
  │    ├─ 该账户处于退避期 → 跳过
  │    ├─ 读 config + cursor（短锁，仅本账户）
  │    ├─ spawn_blocking：连接 → LOGIN（必要时 PLAIN）→ 能力探测 → ID → EXAMINE INBOX
  │    │    ├─ UIDVALIDITY 变化 → lastUid 归零，按首次窗口重扫
  │    │    ├─ 首次：SEARCH SINCE <14 天前>，取最后 100 封
  │    │    └─ 增量：UID FETCH <lastUid+1>:* (UID BODY.PEEK[])，过滤 UID <= lastUid
  │    ├─ 逐封解析 → 映射 → ingest（每封一次短锁）
  │    │    └─ 单封解析失败：跳过并计数，不中断该账户
  │    ├─ 全部处理完才推进该账户 cursor
  │    └─ mark_synced / mark_failed + emit inbox-sync-updated（带该账户 sourceId）
  └─ 整轮预算 5 分钟：到点后剩余账户留到下一轮，已完成的账户状态不受影响
```

账户之间**完全隔离**：A 账户认证失败不影响 B 账户同步，失败只写 A 的 `last_error`。

### 4.3 判断流（完全复用，不改代码）

邮件进入收件箱后与 GitHub 通知一致：`read_at` 表示看过，`triaged_at + triage_outcome` 表示判断过。忽略是软删除，转待办在事务里写 `tasks` 并回填 `triage_ref_id`，重复点击幂等。

### 4.4 事件流

```text
每个账户同步结束 ──emit inbox-sync-updated──▶ AppShell 刷新角标
                                          └─▶ InboxPage 静默重载列表与来源
设置页保存/删除 ──本地重载 list_email_accounts（不发全局事件）
标已读 ──▶ 不产生任何远端动作（邮件不入回写队列）
```

多账户时一轮可能连发多条事件；前端可做 300ms 合并，避免同一轮重载 5 次（非必需，先按现状实现）。

## 5. 接口与事件约定

### 5.1 新增 IPC 命令（4 个，不含同步命令）

| 命令 | 入参 | 出参 | 说明 |
| --- | --- | --- | --- |
| `list_email_accounts` | 无 | `Vec<EmailAccountStatus>` | 设置页与来源初始化；密码永不返回；空数组表示未配置 |
| `test_email_account` | `EmailAccountInput` | `EmailAccountProbe` | 只连接验证，不落库 |
| `save_email_account` | `EmailAccountInput` | `EmailAccountStatus` | 派生 id、校验上限、试连、落配置、写凭据、触发首轮同步 |
| `delete_email_account` | `{ id }` | `Vec<EmailAccountStatus>` | 删凭据、`enabled=0`、清配置与游标，**不删消息** |

不提供 `sync_inbox_email`：与 GitHub 一致，拉取只由后台负责，界面不放同步按钮。

### 5.2 类型契约

```rust
// domain/inbox.rs
pub struct EmailAccountStatus {
    pub id: String,                    // source-email-<uuid>
    pub address: String,
    pub host: String,
    pub port: u16,
    pub tls: EmailTls,                 // implicit | starttls
    pub mailbox: String,
    pub enabled: bool,
    pub last_synced_at: Option<String>,
    pub last_error: Option<String>,
}

pub struct EmailAccountProbe {
    pub mailbox_total: u32,            // EXAMINE 返回的 EXISTS
    pub uid_validity: u32,
}
```

```ts
// src/lib/api.ts
export type EmailTls = "implicit" | "starttls";

export interface EmailAccountStatus {
  id: string;
  address: string;
  host: string;
  port: number;
  tls: EmailTls;
  mailbox: string;
  enabled: boolean;
  lastSyncedAt: string | null;
  lastError: string | null;
}

export interface EmailAccountInput {
  address: string;
  /** 选填：编辑端口 / 加密方式时留空表示沿用已保存的授权码。 */
  password?: string;
  host: string;
  port: number;
  tls: EmailTls;
  mailbox?: string | null;
}
```

入参沿用 `dto.rs` 的 `deny_unknown_fields` + `required_text` / `optional_text` 约定，上限：

| 字段 | 规则 |
| --- | --- |
| `address` | 必填，≤320，必须含 `@`，不含空白与换行 |
| `password` | 选填；新增账户、换授权码或首次测试连接时必填，≤512，只去首尾空白（授权码内部可能有空格）；留空则读该账户已保存的凭据 |
| `host` | 必填，≤253，只允许主机名，不接受 `imap://`、`host:port` |
| `port` | 1..=65535，默认 993 / 143 |
| `tls` | 枚举，默认 `implicit` |
| `mailbox` | 选填，默认 `INBOX`，≤128，拒绝控制字符 |

### 5.3 配置与凭据契约

`inbox_sources.config_json`（非敏感，可读可展示）：

```json
{
  "address": "me@qq.com",
  "host": "imap.qq.com",
  "port": 993,
  "tls": "implicit",
  "mailbox": "INBOX",
  "initialDays": 14,
  "maxMessagesPerSync": 100
}
```

Windows 凭据管理器（service = `TomatoDesk`）：

| 键 | 值 |
| --- | --- |
| `github-token` | 现有 GitHub Token（保持不变） |
| `email-password:source-email-<uuid>` | 该账户的授权码 / 应用专用密码 |

`secrets` 模块把「用户名 = 常量」改成 `fn entry(user: &str)`，GitHub 三个函数保持签名不变，新增 `save_email_password(source_id, password)` / `get_email_password(source_id)` / `delete_email_password(source_id)`。

### 5.4 游标契约

每个账户各有一条游标，存 `inbox_sources.cursor`：

```json
{ "uidValidity": 1234567890, "lastUid": 4521 }
```

规则：

- **只在整轮处理完成后写入**，取值是该账户本轮实际处理过的最大 UID；中途失败不推进。
- 读不出 / 字段缺失 / 类型不符 → 视为无游标，走首次窗口。
- `uidValidity` 与服务器当前值不一致 → 丢弃 `lastUid`，按首次窗口重扫；重复消息由 `(source_id, external_id)` 消掉。
- 首次窗口：`SEARCH SINCE <今天-14 天>`，取结果中最后 100 个 UID（按 UID 升序），避免新装即拉全库。

### 5.5 消息字段映射

| `InboxMessage` 字段 | 邮件来源 | 规则 |
| --- | --- | --- |
| `external_id` | `Message-ID` | 去掉首尾空白与 `<>`；为空或 >512 → `uid:<uidValidity>:<uid>` |
| `category` | 固定 | `mail` |
| `title` | `Subject` | RFC 2047 解码；空 → `(无主题)`；≤512 |
| `summary` | 正文预览 | 纯文本前 200 字，压缩连续空行；正文为空 → `NULL` |
| `body` | 正文 | 优先 `text/plain`；只有 HTML 时转纯文本；≤65536 字符 |
| `url` | 无 | 恒为 `NULL`（不造 `mailto:`，避免误开客户端） |
| `actor` | `From` | `显示名 <地址>`，无显示名时只留地址；清换行与控制字符；≤256 |
| `occurred_at` | `Date` → `INTERNALDATE` → 当前时间 | 转 UTC、RFC3339 毫秒；未来 >24h 或早于 1970 用当前时间 |
| `external_updated_at` | 同 `occurred_at` | 每封信固定，重新同步不会把已读改回未读 |
| `related_kind` / `related_id` | 无 | `NULL`；会话聚合留到后续 |
| `payload_json` | 轻量元数据 | `uid / uidValidity / messageId / from / to / cc / size / attachments`，**不存原始 MIME** |

### 5.6 事件契约

沿用 `inbox-sync-updated`，不改载荷结构：

```ts
interface InboxSyncSummary {
  sourceId: string;      // "source-email-<uuid>"
  fetchedCount: number;  // 本轮取到的封数
  ingestedCount: number; // 实际写入/更新的封数
  pendingCount: number;  // 全局待判断数
  syncedAt: string;
}
```

- 成功发 `Some(summary)`，失败发 `null`（错误已写进该账户的 `last_error`）。
- 未配置 / 被禁用 / 无凭据时**不发事件**，避免前端为一次空同步重载。
- 不新增事件类型；`inbox-changed` 仍只由用户动作触发。

### 5.7 错误契约

命令层只返回既有四类 `code`：参数问题 `validation`、找不到 `not_found`、冲突 `conflict`、其余 `internal`。同步错误写进 `inbox_sources.last_error`，文案面向用户，不含协议细节堆栈。

## 6. 异常与边界处理

### 6.1 连接与认证

| 场景 | 判定 | 处理 | 用户可见 |
| --- | --- | --- | --- |
| DNS / 连接超时 / TLS 握手失败 | `Unreachable` | 下一轮重试，不动游标 | 「无法连接 imap.qq.com:993：连接超时」 |
| 登录被拒（`AUTHENTICATIONFAILED`、`LOGIN failed`） | `Auth` | 该账户内存退避 60 分钟再试，不清配置 | 「登录失败，请确认已开启 IMAP 并使用授权码」 |
| 服务器不支持 IMAP / 端口写错 | `Unreachable` + 提示 | 同上 | 「服务器没有响应 IMAP 握手，确认端口与加密方式」 |
| 文件夹不存在 | `Mailbox` | 该账户退避 60 分钟 | 「找不到文件夹 INBOX」 |
| 服务器 5xx / 协议异常 | `Protocol` | 下一轮重试 | 原文摘要 |

退避只在进程内按 source id 记录连续失败次数，不落库：重启后重试一次是合理的，也避免为它加字段。一个账户退避不影响其他账户。

### 6.2 协议与邮箱状态

- **只读打开**：用 `EXAMINE` 而不是 `SELECT`，取信一律 `BODY.PEEK[]`。普通 `BODY[]` 或 `FETCH RFC822` 会把邮件在服务器上标成已读，等于替用户读了信，这是本阶段最需要守住的边界。
- **UID 区间陷阱**：IMAP 的 `UID FETCH <n>:*` 在 `n` 大于当前最大 UID 时仍会返回最后一封。必须按返回的 UID 过滤 `uid > lastUid`，不能只看区间。
- **UIDVALIDITY**：每次 EXAMINE 后比对，变化即全量重扫窗口。
- **163 / 126 的 `Unsafe Login`**：`EXAMINE` 之前必须先发 `ID`，否则服务器直接拒绝——2026-11-26 对 `abeltomato@163.com` 实测，不发 `ID` 时 `EXAMINE INBOX` 返回 `No Response: Unsafe Login`。`ID` 在**登录前**发送即可满足要求（同一账号实测先 `a0 OK ID completed`、再登录、`EXAMINE` 正常），所以连接流程在 `LOGIN` 之前无条件插入一次 `ID ("name" "TomatoDesk" "version" "<pkg version>")`，不依赖 `CAPABILITY` 探测；服务器不支持时回 `BAD`，忽略即可。
- **`UIDNEXT` 不可依赖**：163 的 `EXAMINE` 响应里 `UIDNEXT` 为 `None`（同一轮实测 `UIDVALIDITY=Some(1)`、`UIDNEXT=None`）。因此游标 `lastUid` 只能取本轮实际处理过的最大 UID，不能用 `UIDNEXT - 1` 推算。
- **认证方式**：优先 `LOGIN`；只有当服务器回 `BAD`（不认识 `LOGIN` 这条命令）时才用 `AUTHENTICATE PLAIN` 回退。凭证被拒（`NO`）时不回退——重试没有意义，还会触发服务商风控。不做 `XOAUTH2`。`imap 2.4.1` 的 `Client` 在未认证态没有 `capabilities()`，因此回退判断只能看错误类型，不能先探能力。
- **QQ / 163 / Gmail**：必须用授权码 / 应用专用密码，不能填登录密码；设置页提示文案写清楚，错误提示也指向这一点。
- **Outlook / Microsoft 365**：官方已关闭 IMAP 基本认证，本方案不支持，文档与设置页明确标注。

### 6.3 解析与编码

- `mail-parser` 开启 `full_encoding`（走 `encoding_rs`）以支持 GBK / GB18030 / Big5 等中文编码；解码失败走有损替换，不丢弃整封。
- 主题与发件人的 RFC 2047 编码字由解析器解码；解码后再次清洗换行、制表符与 `\u{0}`。
- 正文选择顺序：`text/plain` → `text/html` 转纯文本 → 空。`multipart/signed` 取第一段；`multipart/report`（退信）按普通邮件处理。
- HTML 只做「转纯文本」，**不渲染进 WebView**：`script` / `style` / `head` 整段丢弃，块级元素转换行，链接保留锚文本（不保留 href，避免误点）。
- 附件不下载：只统计数量与文件名，正文为空时用「（无正文，含 N 个附件：a.pdf、b.png）」占位。
- 超大邮件：整封 `BODY.PEEK[]` 取回后，由解析器只读前 256 KB（超出部分截断），正文最终截断到 65536 字符；`payload_json` 控制在 4 KB 内。**不能**用 `BODY.PEEK[]<0.262144>` 做 partial fetch：`imap-proto 0.10.2` 的序号解析器只认 `<n>`（单数字），收到 `<0.262144>` 会 `ParseError`，且残留状态行会让下一条命令 tag 断言 panic（2026-11-27 已用 `parse_response` 实测）。因此改为「分批整封取 + 本地截断」，每批 20 封以约束峰值内存。
- 日历邀请（`text/calendar`）、加密邮件（`multipart/encrypted`）没有可读正文时按「无正文」处理，不尝试解密。

### 6.4 容量与时间预算

- 单账户单轮最多处理 100 封，**增量**取最旧的一批（游标停在本轮处理的最大 UID，下一轮继续，不会漏也不会重复）；**首次**取最新的一批（首屏不出现陈年旧信，更早的窗口内邮件不再补拉，见 5.4）。游标只在整轮成功后推进。
- 单账户：连接超时 8s、单条命令读写超时 15s、单轮预算 60s；整轮（全部账户）预算 5 分钟，到点后剩余账户留到下一轮。预算由客户端在阻塞内部逐批检查——`spawn_blocking` 无法从外部打断，只能自己收手。
- 单封解析上限 256 KB、正文 64 KB、摘要 200 字，避免一封巨型邮件拖慢整轮或撑大数据库。
- 账户数上限 5，最坏情况一轮串行 5 个账户仍在 5 分钟预算内。

### 6.5 写入与一致性

- `ingest` 是幂等的：同一 `Message-ID` 重复同步只更新字段，不新增行。
- 已判断的消息不因重新同步恢复未读，判断结果不被覆盖（现有 SQL 已保证）。
- 单封失败不影响其他封；某账户写库失败只中止该账户本轮且**不推进其游标**，其他账户照常。
- 时间统一存 UTC RFC3339；展示交给前端 `formatDate` / `formatRelative`。

### 6.6 生命周期

| 场景 | 处理 |
| --- | --- |
| 新增账户 | 派生 id，校验上限 5，试连成功后写配置与凭据；立即同步一次 |
| 删除账户 | 删凭据 → `enabled = 0` → 清 `config_json` 与 `cursor` → 保留消息（不删 source 行，避免 CASCADE 清空） |
| 重新添加同一账户 | 复用同一 id 与来源行，`enabled=1`、游标为空 → 按 14 天窗口重扫；老消息靠 Message-ID 去重，不会重复 |
| 修改服务器 / 地址 | 视为新账户；设置页编辑时锁住这两个字段，要求先删除旧账户 |
| 停用 / 未配置 | 轮询整轮跳过该账户，不请求、不发事件、不改 `last_error` |
| 某账户断网 | 只记该账户 `last_error`，游标不动，恢复后从原位置继续；其他账户不受影响 |
| 长期未启动 | 首次窗口之外新增的邮件按 UID 逐轮追赶，单账户 100 封封顶 |
| 服务器已删除的邮件 | 本地保留（收件箱是事件记录，不做镜像删除） |
| 密码失效 / 改密 | `Auth` 错误 + 提示重新保存授权码；旧凭据保留，保存新值即覆盖 |

## 7. 可执行接入方案

按「契约 → 解析 → 同步 → 命令 → 界面 → 门禁」推进，每步都能独立验证。

### S0 依赖与技术验证

在 `src-tauri` 下用 `cargo add` 引入，版本交给 cargo 解析并锁进 `Cargo.lock`：

```bash
cargo add imap                     # 默认启用 tls + native-tls，Windows 走 schannel
cargo add mail-parser --features full_encoding
cargo add uuid --features v5       # 已有 v4，补 v5 做确定性 id
cargo add tokio --features rt      # 现有 features 为 time，补 rt 以使用 spawn_blocking
```

- `imap` 的默认 feature 已经是 `tls` → `native-tls`，不需要手动关默认特性；Windows 上走 schannel，不引入 OpenSSL。
- `mail-parser` 的 `full_encoding` 才带 `encoding_rs` 的多字节编码支持（GBK / GB18030 / Big5），必须显式打开。
- 与 reqwest 的 rustls 并存会增加约 1 MB 体积，可接受；若后续要统一 TLS 栈，再按 `imap` 的 rustls 示例替换连接构造。
- 验证：写一个 `#[ignore]` 的集成测试或临时 example，用真实账号跑通「连接 → 登录 → EXAMINE → 取一封」，再补一个 163 账号验证 `ID` 命令。

**已落地（2026-11-26）**

- 版本：`imap 2.4.1`、`mail-parser 1.0.2`、`uuid 1.29.3`（`v4` + `v5`）、`native-tls 0.2.18`、`tokio 1.54.1`（`time` + `rt`）。
- `imap` 默认 feature 就是 `tls` → `native-tls`；Windows 实际编译的是 `schannel`，没有编译 `openssl-sys`（`Cargo.lock` 里的 openssl 是非 Windows 目标的解析产物）。
- 新增 `src-tauri/tests/email_probe.rs`：TLS 握手 → 登录 → `CAPABILITY` → `ID` → `EXAMINE` → 取一封邮件头交给 `mail-parser`。默认 `#[ignore]`；不设账号时只验证握手，供受限网络下确认可达性。
- 已验证：`imap.qq.com:993` TCP 可达、TLS 握手与服务器问候通过；`mail-parser` 的 `full_encoding` 能正确解 GBK 与 UTF-8 的 RFC 2047 主题（`mail_parser_decodes_gbk_and_utf8_subjects`）；`imap.163.com:993` 的 TLS 握手、问候正常，且服务器接受**登录前**的 `ID`（`* ID ("name" "Coremail Imap" …)` + `a0 OK ID completed`）。
- 已闭环（真实账号 `abeltomato@163.com`，2026-11-26 22:24）：场景 B（登录前发 `ID`）全链路成功——问候 `* OK Coremail System IMap Server Ready(...)`、`* ID ("name" "Coremail Imap" …)`、`a0 OK ID completed`、登录成功、`EXAMINE INBOX：2 封，UIDVALIDITY=Some(1)`、首封主题 `Some("新设备登录提醒")`、`Message-ID` 正常；场景 A（不发 `ID`）`EXAMINE` 返回 `No Response: Unsafe Login`，证明 163 强制 `ID`。
- 由此确定的实现口径：`ID` 放在 `LOGIN` 之前无条件发送；`EXAMINE` 返回的 `UIDNEXT` 在 163 上为 `None`，游标不能依赖它；`Unsafe Login` 以 `Error::No` 形式出现，`EmailError` 需要单独识别并映射成「请检查授权码或客户端不被支持」。
- 复跑方式（PowerShell；包装脚本会吞掉裸 `--`，这里直接调 cargo）：

```powershell
$env:CARGO_TARGET_DIR = Join-Path $env:TEMP "TomatoDesk\cargo-target"
$env:TOMATODESK_IMAP_HOST = "imap.163.com"
$env:TOMATODESK_IMAP_USER = "you@163.com"
$env:TOMATODESK_IMAP_PASS = "授权码"
cargo test --manifest-path src-tauri\Cargo.toml probe_imap -- --ignored --nocapture
Remove-Item Env:TOMATODESK_IMAP_PASS
```

- API 事实（写 S3 时直接用）：`Client::login(self, ...)` 消费 client，失败返回 `(Error, Client)`；`Session::capabilities()` 用 `has_str("ID")` 判断；`examine()` 是只读打开；`fetch(..., "BODY.PEEK[HEADER]")` 的结果用 `Fetch::header()` 取；`Message::message_id()` 已去掉尖括号。

**连接构造的定稿（2026-11-26 实测修正）**

不要用 `imap::connect` + `Session::run_command_and_read_response("ID …")`。原因是两层叠加：

1. `imap 2.4.1` 的 `run_command_and_read_response` 把每一行交给 `imap-proto 0.10.2` 解析，而后者不认识 `* ID (...)` 这个 RFC 2971 未标记响应，直接返回 `ParseError::Invalid`（显示为 `Unable to parse status response`）；
2. 解析失败后 `aN OK` 没有被消费，`Connection::read_response_onto` 里 `assert_eq!(tag, match_tag)` 会在**下一条命令**上炸掉（实测 `left: a3, right: a4`），错误现场与真正原因完全错位。

更重要的是，`imap::connect` 不提供超时设置，后台轮询一旦遇到半开连接就会永久挂住。因此统一改成：

```
TcpStream::connect_timeout(8s)
  → set_read_timeout / set_write_timeout(15s)
  → native_tls 握手
  → 自己逐字节读问候行（不能套 BufReader，会预读走后续响应）
  → 可选：写 "a0 ID (\"name\" \"TomatoDesk\" \"version\" …)\r\n"，自己读到 a0 状态行
  → imap::Client::new(stream) 接管，tag 从 a1 开始，不会与 a0 撞号
  → client.login(...) → Session
```

- `ID` 放在**登录前**发送：RFC 2971 允许 ID 处于非认证态，163 实测接受（`a0 OK ID completed`），于是完全不需要碰 `run_command_and_read_response`。
- 问候被自己消费后不能再调 `Client::read_greeting()`（它 `assert!(!greeting_read)`），直接 `login` 即可——`greeting_read` 只用于这条断言，`login` 不检查它。
- 对不宣告 `ID` 的服务器（Gmail、iCloud、QQ）发 `ID` 会得到 `BAD`，按预期忽略，不影响后续登录。
- 这套构造直接进 `infrastructure/email/imap_client.rs`，探针里已经跑过（`src-tauri/tests/email_probe.rs` 的场景 B）。

### S1 契约与存储层

1. `infrastructure/secrets`：抽出 `entry(user)`，新增三个邮箱凭据函数，GitHub 三函数行为不变。
2. `repositories/inbox_source.rs`：新增 `upsert_email(id, name, config_json)`（插入或更新配置并置 `enabled=1`）、`list_email()`（返回 id/name/config/enabled/last_synced/last_error）、`config(id)`、`save_config(id, json)`、`set_enabled(id, bool)`、`clear_cursor(id)`、`count_configured_email()`。
3. `domain/inbox.rs`：新增 `EmailTls` / `EmailAccountStatus` / `EmailAccountProbe`。
4. `infrastructure/email/mod.rs`：`EmailAccount`（id/address/host/port/tls/mailbox）、`account_id(address, host)`（UUID v5）、`EmailError` 及 `Display`。

验收：仓储测试覆盖 upsert 幂等、配置往返、软停用、游标清理、账户计数；`cargo test` 不触网。

**已落地（2026-11-26 22:45）**

- `infrastructure/secrets/mod.rs`：抽出 `entry(user)` / `save_secret` / `get_secret` / `delete_secret`，GitHub 三个函数签名与文案不变；新增 `save_email_password` / `get_email_password` / `delete_email_password`，键名 `email-password:<source_id>`。空 `source_id` 直接返回，不碰凭据存储。
- `domain/inbox.rs`：新增 `EmailTls`（`implicit` / `starttls`，`default_port()` 给 993 / 143）、`EmailAccountStatus`（无密码字段）、`EmailAccountProbe`（`mailbox_total` + `uid_validity`）。
- `infrastructure/email/mod.rs`：`EmailConfig`（`config_json` 的强类型，含 `initialDays` / `maxMessagesPerSync` 默认值）、`EmailAccount`（内部持有 `EmailConfig`，暴露 `address()` / `host()` / `port()` / `tls()` / `mailbox()` / `display_name()`）、`account_id(address, host)`（UUID v5，命名空间串固定）、`is_email_account_id`、`EmailError`（`Unreachable` / `Auth` / `Mailbox` / `Protocol` + `needs_backoff()` + `is_unsafe_login()`）、`MAX_EMAIL_ACCOUNTS = 5`。
- `repositories/inbox_source.rs`：新增 `EmailSourceRow`、`upsert_email`、`list_email`、`config`、`save_config`、`set_enabled`、`clear_cursor`、`count_configured_email`。
- 两个实现取舍：`upsert_email` 冲突时**清空游标并清 `last_error`**（改地址/主机/文件夹后旧 UID 可能失效，靠 Message-ID 去重兜底）；`count_configured_email` 只数 `enabled = 1`，软停用的账户不占上限。
- 测试：`secrets` 只验证键名派生（真实读写会污染用户凭据库）；`EmailConfig` 覆盖往返、默认文件夹、坏 JSON；`account_id` 覆盖归一化、区分主机与地址、拒绝异类 id；仓储覆盖 upsert 幂等、配置往返、游标清空、软停用后消息保留、多账户互不覆盖。
- 验证：`cargo fmt --check`、`clippy --all-targets -D warnings`、`cargo test --locked`（58 通过 + 探针 1 ignored）均通过，未触网。

### S2 解析层与 fixtures

1. `infrastructure/email/parse.rs`：`parse_message(raw: &[u8]) -> ParsedMail`，包含主题/发件人/日期/正文/附件名/Message-ID 的提取与清洗，全部是纯函数。
2. `src-tauri/tests/fixtures/email/` 放 7 个 `.eml`：纯文本、GBK 主题与正文、只有 HTML（含追踪像素）、`multipart/alternative` + 附件、缺 `Message-ID`、`Date` 非法、`multipart/signed`。
3. `application/inbox/email.rs` 的 `to_new_message(parsed, source_id) -> NewInboxMessage`：实现 5.5 的映射与截断。

验收：单测覆盖 6.3 / 6.4 的每一条规则；fixtures 全部能解析出非空标题与稳定 `external_id`。

**已落地（2026-11-26 23:42）**

- `infrastructure/email/parse.rs`：`parse_message(raw) -> ParsedMail` 纯函数，字段为 `subject` / `from` / `from_address` / `to` / `cc` / `date_unix` / `message_id` / `body` / `attachment_names` / `server`。
- 清洗规则：单行字段（主题、发件人、附件名）把空白与控制字符压成单空格；正文统一 CRLF→LF、去掉其余控制字符、三个以上连续换行压成两个。
- 上限常量集中在 `parse.rs`：原文 256 KB、正文 64 K 字符、标题 512、发件人 256、`Message-ID` 512、附件名 256、`payload_json` 4 KB、收件人 20 个。
- `html_to_text` 自带 `head` / `style` / `script` / `template` 剔除且只保留锚文本，正好满足 6.3 的要求，不需要自己写正则清洗。
- `application/inbox/email.rs`：`to_new_message(source_id, parsed, now)` 实现 5.5 的映射。`occurred_at` 用 `Date`，超前 24 小时或早于 1970 时留空交给 SQL 填收到时间；`external_updated_at` 与之同值，避免每轮同步都变大而把已读翻回未读；正文为空但有附件时写「（无正文，含 N 个附件：…）」占位；`payload_json` 超过 4 KB 时丢掉收件人列表。
- 7 个 fixtures：`plain`、`gbk`（GBK 主题 + GBK Base64 正文）、`html-only`（含 `script`、`style`、追踪像素）、`multipart-alternative-attachment`（纯文本优先 + `report.pdf`）、`no-message-id`、`bad-date`（`32 Nov 2026 99:99:99`）、`multipart-signed`。
- 测试：解析层 11 个用例（含 fixtures 全量遍历与「7 个样本」数量断言、超长原文截断、非邮件字节降级不 panic），映射层 8 个用例（时间归一、UID 兜底、未来日期拒绝、占位文案、摘要 200 字、payload 预算）。`cargo test --locked` 77 通过 + 探针 1 ignored。
- 一处待 S3 衔接：`date_unix` 在 `Date` 缺失时由调用方用 `INTERNALDATE` 填入，这样每封信的时间取值仍然稳定。

### S3 同步与轮询

1. `infrastructure/email/client.rs`：`probe(account, password)`、`fetch_new(account, password, cursor) -> FetchOutcome`（`Mailbox` 头信息 + `Vec<ParsedMail>` + `next_cursor`）。整体同步阻塞，调用方用 `spawn_blocking`。
2. `application/inbox/email.rs`：`sync_account(database, account)`、`sync_all(database)`（串行遍历 + 整轮预算）、`start_poller(database, app)`（10 分钟、首轮立即、`MissedTickBehavior::Delay`）、按 source id 的内存退避表。
3. `lib.rs` 的 `setup` 注册 `start_poller`；与 GitHub 轮询、回写 worker 并列。

验收：无账户时整轮零请求；一个账户失败不影响另一个账户；断网时 `last_error` 有值且游标不变；连续两轮同步不产生重复行；`UIDVALIDITY` 变化后能重扫且不重复。

**已落地（2026-11-27）**

- `infrastructure/email/client.rs`：`MailCursor`（`uidValidity` / `lastUid`，坏 JSON 与 `lastUid = 0` 视为无游标）、`FetchOutcome`、`probe`、`fetch_new(account, password, cursor, deadline)`。
- 建连：`TcpStream::connect_timeout(8s)` + 15s 读写超时 → `implicit` 直接 native-tls / `starttls` 先明文读问候、发 `s0 STARTTLS`、再在同一连接升级 → 自读问候 → 登录前发 `a0 ID` → `Client::new` 接管。`STARTTLS` 与 `ID` 用不同 tag（`s0` / `a0`），`Client` 的计数器从 `a1` 起，三者不撞号。
- 认证：`LOGIN` 失败且错误是 `BAD` 时才回退 `AUTHENTICATE PLAIN`；`NO`（凭证被拒、`Unsafe Login`）直接按认证问题返回并退避，不重试。
- 取信：`EXAMINE`（只读）→ 增量 `UID <last>:*` 后按 `uid > last` 过滤（区间陷阱）→ 首次 `SINCE <今天-14d>` → 排序后按 5.4 选批 → 每 20 封一批 `UID FETCH (UID RFC822.SIZE INTERNALDATE BODY.PEEK[])` → 逐封解析并回填 `server` 元数据、`Date` 缺失时用 `INTERNALDATE`。
- 游标：只在整批取回后前移，取值是本轮实际处理的最大 UID；`UIDNEXT` 完全不参与（163 为 `None`）。
- `application/inbox/email.rs`：`MailFetcher` trait（生产实现 `ImapFetcher` 转发到客户端，测试用假实现）、`sync_account`（读游标 → 取信 → 逐封 `ingest` → `mark_synced`；任一步失败写 `last_error` 且不写游标）、`sync_all`（串行 + 5 分钟整轮预算 + 60 秒单账户预算 + `spawn_blocking`）、`Backoff`（进程内、按 source id、60 分钟）、`start_poller`（10 分钟、首轮立即、`MissedTickBehavior::Delay`）。删掉了 `to_new_message` 的 `allow(dead_code)`。
- `lib.rs` 的 `setup` 注册 `start_poller`，与 GitHub 轮询、回写 worker 并列。
- 事件：成功每个账户发一次 `Some(summary)`，失败发 `null`；无启用账户、无凭据、配置损坏、退避中都**不发事件**。
- 一处实测修正：`BODY.PEEK[]<0.262144>` 会让 `imap-proto 0.10.2` 解析失败（只认 `<n>`），partial fetch 不可用，已改为分批整封取（见 6.3）。这也意味着单封 5 MB 邮件会完整读进内存一次，靠批大小 20 约束峰值。
- 测试：`client` 8 个用例（游标记往返与坏值、首次取最新 / 增量取最旧、日期格式、错误分类、状态行大小写）+ 编排 10 个用例（写库与游标、游标回传、两轮不重复、认证失败保留游标并退避、网络失败只重试、`UIDVALIDITY` 变化重扫不重复、账户隔离、空账户零请求、停用账户不轮询、退避时效）。`cargo test --locked` 96 通过（新增 19 个），探针 1 ignored；`fmt --check` 与 `clippy --all-targets -D warnings` 通过。
- 未闭环：真实账号下的端到端同步仍需手工验证（本地无账号，自动化只能覆盖到取信接口的假实现）。

### S4 命令层

1. `commands/email.rs`：四个命令的 `*_in(database, input)` 版本 + `#[tauri::command]` 包装，便于测试。
2. `commands/dto.rs`：`EmailAccountInput`、`IdInput` 复用，按 5.2 的校验规则。
3. `commands/error.rs`：新增 `EmailError -> CommandError` 的映射（`Auth` / `Mailbox` / `Unreachable` 归 `validation`，其余 `internal`）。
4. `lib.rs` 注册四个命令。

验收：空地址、无 `@`、端口越界、`host` 带协议前缀都返回 `validation`；第 6 个账户返回 `validation`；`list_email_accounts` 在任何情况下都不含密码字段。

**已落地（2026-11-27）**

- `commands/email.rs`（新增）：`list_email_accounts_in` / `test_email_account_in` / `save_email_account_in` / `delete_email_account_in` + 四个 `#[tauri::command]` 包装。保存顺序是「校验 → 派生 id → 上限检查 → 试连 → 写凭据 → upsert 来源」，试连失败不落库、不覆盖旧凭据。
- 两个可替换接口，避免命令层测试触网或写入真实凭据库：`MailProber`（生产 `ImapProber`，放在 `application::inbox::email`，与 `MailFetcher` 同层）和 `EmailCredentials`（生产 `KeyringCredentials`，命令层私有）。
- 阻塞操作统一走 `run_blocking`（`tokio::task::spawn_blocking`），异步命令不占住运行时。
- 保存成功后调用 `application::inbox::email::sync_account_soon` 立即同步该账户：`sync_all` 抽出 `sync_selected(only)` 复用同一套预算与事件逻辑，单账户同步用独立退避表，用户刚改完配置不应被上一次失败拦住。
- `commands/dto.rs`：新增 `EmailAccountInput` → `EmailAccountForm`。地址 ≤320 且必须含 `@`、不含空白；主机 ≤253 且拒绝 `://`、`:`、`/`；端口缺省按加密方式补 993/143、`0` 拒绝；`mailbox` ≤128 且拒绝控制字符；`password` ≤512 只去首尾空白。
- `commands/error.rs`：`From<EmailError>` → `Unreachable` / `Auth` / `Mailbox` 归 `validation`，`Protocol` 归 `internal`，文案直接用 `EmailError` 的中文说明。
- 三处实现决策（与原稿的差异，已同步到 5.2）：
  1. **`password` 改选填**：新增账户或换授权码时必须填，编辑端口 / 加密方式时留空即沿用已保存凭据——否则设置页每次改端口都要用户重新找授权码。
  2. **删除账户把 `config_json` 写成 `{}`**：列上是 `NOT NULL DEFAULT '{}' CHECK (json_valid(config_json))`，空串会触发 `SQLITE_CONSTRAINT`（映射成 `conflict`），`NULL` 又会让按 `String` 取列的 `list_email` 报 `InvalidColumnType`。空对象两处都满足，`EmailConfig::parse` 判为未配置，账户从列表消失但来源行与消息保留。仓储新增 `clear_config`，注释写明原因。
  3. **上限判断只拦「新增」**：同一账户重复保存（`enabled = 1` 且 id 相同）不占新名额，改端口不会因为满 5 个而失败。
- 测试：`commands/email.rs` 内联 9 个用例——入参校验矩阵、端口默认值、重复保存复用一行、第 6 个账户被拒且删除后可再加、删除后消息保留且账户隐藏、序列化结果不含 `password`、无凭据时不发请求、认证失败映射为 `validation` 且不落库、非邮箱 id 返回 `not_found`。
- 验证：`cargo test --locked` 105 通过（新增 9 个），探针 1 ignored；`fmt --check`、`clippy --all-targets -D warnings` 通过，全部不触网。
- 门禁口径修正：`scripts/windows-cargo.ps1` 的 `-Command` 补了 `fmt`（原来只允许 check/build/clippy/test，`fmt --check` 会被参数校验直接拒绝）；clippy 带 `-- -D warnings` 时 PowerShell 会把 `--` 后的内容绑错，实际用 `-Command clippy -CargoArgs "--all-targets","--","-D","warnings"`。
- 未闭环：真实账号下的保存 → 首轮同步仍待 S5 界面接好后手工验证；`sync_account_soon` 只能通过真实 `AppHandle` 触发，自动化只覆盖到 `sync_account` 与命令层假实现。

### S5 前端

1. `src/lib/api.ts`：四个调用 + 类型 + preview 桩（`fixtures` 里维护一份可变账户列表）。
2. `SettingsPage.tsx`：新增「邮箱账户」`Section`，位于 Token 与已读回写之间：
   - 无账户：直接展示新增表单（地址、授权码、服务器、端口、加密方式），默认端口随加密方式切换。
   - 有账户：列表展示地址、服务器、上次同步、最近错误；每行「编辑」（只改端口/加密/文件夹）与「删除」（二次确认，提示「历史消息会保留在收件箱」）；底部「添加账户」，达到 5 个时禁用并说明上限。
   - 「测试连接」对当前表单生效，成功显示邮箱封数与 UIDVALIDITY，不做持久化。
   - 常见服务商预填（qq.com → imap.qq.com:993，163.com → imap.163.com:993，gmail.com → imap.gmail.com:993，icloud.com → imap.mail.me.com:993），只填主机与端口。
3. `InboxPage.tsx`：头部「自动同步」取**已启用来源中最新的 `lastSyncedAt`**；来源栏提示补一句邮箱同步说明；空状态文案去掉「邮件连接器随后提供」。
4. `InboxMessageDetail.tsx`：「相关」区按来源分支——GitHub 保持现状，邮件显示「邮件原文已保存在收件箱，回复请到邮箱客户端」且不出现 GitHub 按钮。
5. `fixtures.ts`：加两个邮箱来源（如 QQ 与 163）与一封邮件消息（`category = "mail"`、`url = null`、有 `body`），保证 `?preview` 下多来源、三栏与详情可走查。

验收：`?preview&view=settings` 与 `?preview&view=inbox` 在 1440×900、820×900 下布局正常；两个邮箱账户在来源栏可分别筛选；邮件详情的按钮集合与 GitHub 通知不同且没有死链。

### S6 门禁与文档

1. 跑完整门禁（见 9.4）。
2. 更新本计划的状态与落地记录、当天的开发日志；发布说明标注「多账户上限 5、仅支持授权码、不支持 Outlook/M365、已读不回写服务器、不下载附件」。
3. 计划文档 8 的阶段五条目改为「已实施」，并链到本文件。

## 8. 涉及文件与模块

| 文件 | 变化 |
| --- | --- |
| `src-tauri/Cargo.toml` | 新增 `imap`、`mail-parser`；`uuid` 补 `v5`；tokio 补 `rt` |
| `src-tauri/src/infrastructure/email/mod.rs` | 新增：账户配置、确定性 id、错误类型 |
| `src-tauri/src/infrastructure/email/client.rs` | 新增：IMAP 会话、能力探测、探测与增量取信 |
| `src-tauri/src/infrastructure/email/parse.rs` | 新增：MIME 解析、编码、HTML→纯文本 |
| `src-tauri/src/infrastructure/mod.rs` | 导出 `email` |
| `src-tauri/src/infrastructure/secrets/mod.rs` | 抽出 `entry(user)`，加邮箱凭据三函数 |
| `src-tauri/src/application/inbox/email.rs` | 新增：测试连接替换点 `MailProber`、多账户同步、映射、轮询、退避、游标、单账户立即同步 |
| `src-tauri/src/application/inbox/mod.rs` | 声明 `email` |
| `src-tauri/src/domain/inbox.rs` | 新增 `EmailTls` / `EmailAccountStatus` / `EmailAccountProbe` |
| `src-tauri/src/infrastructure/db/repositories/inbox_source.rs` | 加 `upsert_email`、`list_email`、配置读写、清配置、启停、清游标、计数 |
| `src-tauri/src/commands/email.rs` | 新增四个命令 |
| `src-tauri/src/commands/dto.rs` | `EmailAccountInput` 与校验 |
| `src-tauri/src/commands/mod.rs` | 导出 `email` |
| `src-tauri/src/commands/error.rs` | `EmailError` 映射 |
| `src-tauri/src/lib.rs` | 注册 4 个命令，`setup` 启动邮件轮询 |
| `src/lib/api.ts` | 邮件账户类型与 4 个调用（含 preview 桩） |
| `src/features/settings/SettingsPage.tsx` | 「邮箱账户」区块（列表 + 表单 + 测试 + 删除） |
| `src/features/inbox/InboxPage.tsx` | 同步时间、来源提示、空状态文案 |
| `src/features/inbox/InboxMessageDetail.tsx` | 「相关」区按来源分支 |
| `src/features/dev/fixtures.ts` | 两个邮箱来源与邮件消息 |
| `src-tauri/tests/email_probe.rs` | 新增：手工联调探针（默认 ignore）与编码用例 |
| `src-tauri/tests/fixtures/email/*.eml` | 新增解析用例样本 |
| `docs/releases/v0.5.0.md` | 发布说明（随版本） |

不需要改动：`0007_inbox_module.sql`、`inbox_message.rs` 的 upsert、`inbox_read_back`、判断动作、收件箱来源栏。

## 9. 验证要点

### 9.1 自动化

**解析层（纯函数，主力）**

- RFC 2047 主题：UTF-8 Base64、GBK Q 编码、混合编码字。
- GBK / GB18030 正文正确解码；声明编码与实际不符时有损替换而不是 panic。
- 只有 HTML 时转纯文本：`script` / `style` 丢弃、块级换行、锚文本保留、追踪像素不残留。
- `multipart/alternative` 取 `text/plain`；`multipart/signed` 取第一段；附件只留文件名。
- 缺 `Message-ID` 时回退 `uid:<uidValidity>:<uid>`；超长或被 `<>` 包裹时归一化正确。
- `Date` 缺失/非法/未来时间/1970 前 → 落到当前时间；正常时间转 UTC 不偏移。
- 长度截断：标题 512、摘要 200、正文 65536、发件人 256，均按字符而非字节。

**账户与同步层**

- 确定性 id：同地址 + 同主机恒等；大小写不同仍恒等；改主机得到不同 id；不同地址不冲突。
- 账户上限：第 5 个成功、第 6 个 `validation`；删除一个后可再加。
- 游标编解码往返；JSON 损坏退化为首次窗口；`uidValidity` 变化触发重扫。
- `UID FETCH n:*` 返回最后一封时按 UID 过滤，不重复入库。
- 单账户超上限：增量只处理最旧的 100 封，游标停在第 100 封，下一轮继续；首次只处理最新的 100 封，更早的不再补拉。
- 单封解析失败被跳过，该账户整轮仍成功，`ingested_count` 如实计数。
- 账户隔离：A 账户 `Auth` 失败时 B 账户仍完成同步，A 的退避不影响 B。

**仓储与命令**

- `upsert_email` 幂等；重复保存同一账户不新增行；配置写入/读取往返。
- 删除账户后消息仍在、来源行仍在、`config_json` 与 `cursor` 为空、`enabled = 0`。
- 重新添加同一账户复用同一 id，按窗口重扫且 Message-ID 去重后无重复行。
- 相同 `Message-ID` 连续 ingest 两次只有一行；已判断的消息不被同步恢复未读。
- 入参校验矩阵：空地址、无 `@`、含空格的主机、`imap://` 前缀、端口 0 / 65536、非法 `tls`。
- 任何返回 `EmailAccountStatus` 的命令都不含密码字段（序列化快照断言）。

### 9.2 手工

1. 用 QQ 邮箱授权码配置：保存即触发首轮，收件箱出现最近 14 天的邮件；重启应用后不重复。
2. 再加一个 163 账户：确认同一轮能同步两个账户，来源栏可分别筛选；163 不出现 `Unsafe Login`。
3. Gmail 应用专用密码：确认可登录；用登录密码时提示「请使用应用专用密码」。
4. 故意让 163 授权码失效：QQ 账户仍正常同步，只有 163 显示错误，且不每 10 分钟刷一次。
5. 发一封新邮件到任一账户，等一轮轮询（10 分钟）确认自动出现且未读。
6. 断网后重启，确认 `last_error` 有值、恢复网络后自动补上，游标没丢。
7. 打开一封邮件后检查网页邮箱：该邮件仍是未读（证明没有写回 `\Seen`）。
8. 删除其中一个账户，确认另一账户继续同步、被删账户历史消息仍在；达到 5 个账户后「添加」被禁用。
9. 转一封邮件为待办，确认待办详情显示「来自邮件」并能回跳原消息。
10. 改端口或加密方式保存，确认复用同一账户而不是新增一行。

### 9.3 性能

- 单账户 100 封 × 平均 100 KB：解析 + 入库 < 3s，UI 无卡顿（入库在后台线程）。
- 5 个账户各 100 封：整轮在 5 分钟预算内完成；超预算时下一轮继续，不堆积。
- 单封 5 MB 带附件：只取前 256 KB，整轮不被拖垮。
- 收件箱累计 1 万封邮件后：待判断计数与分页仍走 partial index，首屏 < 200ms。

### 9.4 门禁命令

```bash
scripts/windows-cargo.ps1 fmt --check
scripts/windows-cargo.ps1 -Command clippy -CargoArgs "--all-targets","--","-D","warnings"
scripts/windows-cargo.ps1 test --locked
pnpm exec tsc --noEmit
pnpm lint && pnpm test && pnpm build && pnpm check:design && pnpm format:check
```

注意：`clippy ... -- -D warnings` 直接写成位置参数时，PowerShell 会把 `--` 之后的内容绑错，必须用上面 `-CargoArgs` 的写法；`fmt` 已在包装脚本的 `ValidateSet` 里。

## 10. 风险与回滚

| 风险 | 影响 | 对策 |
| --- | --- | --- |
| 用 `BODY[]` 取信把服务器邮件标已读 | 用户的邮箱被「替读」 | 只读 `EXAMINE` + `BODY.PEEK[]`；手工验证第 7 条 |
| 163 缺 `ID` 命令 | 无法选箱，功能不可用 | 登录后检测 `CAPABILITY` 发 `ID`；单独手工验证 |
| GBK / 非 UTF-8 邮件乱码 | 内容不可读 | `full_encoding` + 有损回退；fixtures 覆盖 |
| HTML 直接渲染带来注入风险 | XSS / 远程像素追踪 | 只转纯文本，不注入 DOM，不加载图片 |
| 首次同步拉全库 | 首屏慢、数据库暴涨 | 14 天窗口 + 单账户 100 封 + 后续逐轮追赶 |
| 多账户串行超时 | 一轮跑不完，后几个账户饿死 | 整轮 5 分钟预算 + 单账户 60s；下一轮从未处理的账户继续 |
| 同一服务商并发连接被限流 | 登录失败或断连 | 账户串行，不并行；每账户独立退避 |
| 阻塞式 IMAP 卡住运行时 | 界面卡顿 | `spawn_blocking` + 连接 8s / 命令 15s / 单账户 60s 预算 |
| partial fetch 响应解析失败 | 取信报错，且残留状态行会让下一条命令错位 | `imap-proto 0.10.2` 只认 `<n>`，一律整封取 + 本地截断；每批 20 封约束峰值内存 |
| 大附件邮件一次性读入内存 | 单封 5 MB 时短暂占用明显 | 分批取信，解析后只留 64 K 字符正文，原始字节随批次释放 |
| 删除账户误删消息 | 数据不可恢复 | 只软停用，不删来源行（CASCADE 边界） |
| 误改地址/主机产生「影子账户」 | 来源重复、消息分散 | 编辑时锁住地址与主机；派生 id 保证同一账户不重复 |
| 新增依赖导致体积与构建问题 | 安装包变大 | native-tls/schannel，无 OpenSSL；体积增量单独记录 |
| 授权码泄漏 | 账号风险 | 只存 keyring；不入库、不回传、不进日志 |
| 用户用 Outlook/M365 反复失败 | 体验差 | 设置页与文档明确不支持基本认证 |

**回滚**：本阶段不加迁移，回滚只需移除 `setup` 里的 `start_email_poller` 与四个命令、回退前端区块；已入库的邮件消息与来源行可以保留（不影响 GitHub 与判断流程），也可以逐个删除账户（软停用，消息保留）。

需要特别说明降级路径：`Database::migrate` 只补齐「比库里多」的迁移，**不会拒绝更高的库版本**。用旧版二进制打开 v8 数据库不会在启动时报错，但运行时会因为查旧表名而失败。因此任何跨阶段降级仍然必须恢复迁移备份。

## 11. 完成标准

- [ ] 设置页可以添加、编辑、删除最多 5 个邮箱账户，测试连接给出明确成功/失败结果。
- [ ] 保存后无需手动触发，该账户首轮同步自动完成，收件箱出现邮件消息且分类为「邮件」。
- [ ] 多个账户在来源栏可分别筛选；一个账户失败不影响其他账户。
- [ ] 重复同步、应用重启、UIDVALIDITY 变化都不产生重复消息。
- [ ] 邮件详情的动作集合与 GitHub 通知一致（已读 / 忽略 / 转待办），没有 GitHub 死链。
- [ ] 转待办后显示「来自邮件」并可回跳原消息。
- [ ] 收件箱已读不改变邮箱服务器状态（手工验证）。
- [ ] 密码不出现在任何 IPC 返回值、日志或 `payload_json` 中。
- [ ] 全部自动化门禁通过，计划与开发日志补齐。

## 12. 评审记录与后续待定

已确认（2026-11-26）：

- 多账户，上限 5；单账户是上限为 1 的特例。
- 已读不回写服务器。
- 首次 14 天窗口、单账户单轮 100 封。
- 只取 `INBOX`。
- 不支持 Outlook / M365 与 NTLM。
- 删除账户为软停用，保留历史消息。

后续版本再议：

- OAuth2（XOAUTH2）接入，覆盖 Outlook / M365。
- 附件按需下载与预览。
- 邮件会话聚合（按 `References` / `In-Reply-To`）。
- 已读状态双向同步（需要给回写队列加 provider 维度）。
- 自定义账户显示名、文件夹订阅与过滤规则。

## 13. 进度记录

- 2026-11-26 18:45：方案初稿，覆盖接入位置、数据流、契约与边界，等待评审。
- 2026-11-26 18:55：评审后改为多账户（上限 5），补服务商复杂度矩阵，命令与来源 id 规则同步调整。
- 2026-11-26 19:05：S0 完成。引入 `imap 2.4.1` / `mail-parser 1.0.2`（`full_encoding`）/ `native-tls 0.2.18` / `uuid v5` / `tokio rt`；新增 `tests/email_probe.rs`；`imap.qq.com:993` 的 TCP 与 TLS 握手通过，GBK/UTF-8 主题解码用例通过；登录与 163 的 `ID` 仍待真实账号手工验证。`cargo fmt --check`、`clippy --all-targets -D warnings`、`cargo test --locked`（49 个原有用例 + 1 个编码用例通过，探针 1 个 ignored）均通过。
- 2026-11-26 21:20：163 真实账号实测暴露 `imap` 的 `ID` 缺陷——`imap-proto 0.10.2` 解析 `* ID (...)` 失败后未消费 `aN OK`，下一条命令 tag 断言 panic（`a3` vs `a4`），`EXAMINE` 实际未执行。探针改为双场景并改为自建 TLS 连接：`connect_timeout` + 读写超时 + 自读问候 + 登录前发 `ID` + `Client::new`。实测 163 接受登录前 `ID`（`a0 OK ID completed`），`EXAMINE`/取信待账号验证。
- 2026-11-26 22:30：S0 闭环。`abeltomato@163.com` 双场景实测：先发 `ID` 时 `EXAMINE INBOX` 正常（2 封、`UIDVALIDITY=1`、主题「新设备登录提醒」、`Message-ID` 完整）；不发 `ID` 时 `EXAMINE` 返回 `No Response: Unsafe Login`。结论：`ID` 必须发送且放在登录前；163 不返回 `UIDNEXT`，游标只能取本轮处理的最大 UID。
- 2026-11-26 22:45：S1 完成。凭据键名改为按来源 id 派生，`inbox_sources` 增加邮箱账户的 upsert / 列表 / 配置读写 / 软停用 / 清游标 / 计数，`domain` 与 `infrastructure::email` 补齐账户契约与错误分类；58 个测试通过，探针 1 ignored。
- 2026-11-26 23:42：S2 完成。`parse_message` 纯函数 + 7 个 `.eml` fixtures + `to_new_message` 映射；fixtures 覆盖 GBK、只有 HTML（含追踪像素）、HTML+附件、缺 `Message-ID`、非法 `Date`、`multipart/signed`；77 个测试通过（新增 19 个），探针 1 ignored。
- 2026-11-27 09:42：S3 完成。新增 `infrastructure/email/client.rs`（建连 / STARTTLS / `ID` / `LOGIN`+PLAIN 回退 / `EXAMINE` / UID 增量取信 / 游标 / 错误分类），`application/inbox/email.rs` 补齐 `MailFetcher`、`sync_account`、`sync_all`、`Backoff`、`start_poller`，`lib.rs` 注册轮询。实测确认 `BODY.PEEK[]<0.262144>` 无法被 `imap-proto 0.10.2` 解析，改成分批整封取。96 个测试通过（新增 19 个），探针 1 ignored；`fmt --check`、`clippy -D warnings` 通过。
- 2026-11-27 10:05：S4 完成（`c6e1931`）。新增 `commands/email.rs`（四个命令 + `MailProber` / `EmailCredentials` 两个替换点）、`EmailAccountInput` 校验、`EmailError -> CommandError` 映射、`lib.rs` 注册；`sync_all` 抽出 `sync_selected(only)` 并新增 `sync_account_soon`，保存后立即同步该账户。删除账户改用 `config_json = '{}'`（列上有 `json_valid` CHECK，空串会撞约束；`NULL` 会让 `String` 读取失败），仓储新增 `clear_config`；`password` 改选填以支持「只改端口」。105 个测试通过（新增 9 个），探针 1 ignored，`fmt` / `clippy -D warnings` 通过，全部不触网。
