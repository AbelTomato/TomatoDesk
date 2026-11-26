# 随手记、待办与收件箱的模块拆分方案

- 状态：已确认，阶段一、阶段二、阶段三已实施
- 日期：2026-11-26
- 涉及模块：`items` / `inbox` / `capture` / `task` / `repos` / `search` / `export`
- 目标版本：MVP 为 v0.5.0（阶段一至三），连接器与增强项后续版本推进

## 1. 目标

把当前混在 `items` 里的语义拆成三个独立领域：

1. **随手记（Capture）**：持久化的个人资产。用户主动收集的链接、笔记、零散想法，长期保留、可编辑、可检索，不是待办。
2. **待办（Task）**：需要执行的事情。有完成状态和来源，回答「接下来要做什么」。
3. **收件箱（Inbox）**：外部事件与事务处理的入口。GitHub 通知、邮件等进来后由人判断：忽略、转为待办，或提取为随手记（后续）。

三个领域对应三个动作：

| 领域   | 回答的问题         | 核心动作 |
| ------ | ------------------ | -------- |
| 收件箱 | 这条消息要不要管？ | 判断     |
| 待办   | 接下来做什么？     | 执行     |
| 随手记 | 什么值得长期留下？ | 沉淀     |

把这三件事塞进一张表和一个状态字段，正是当前 `items.status` 的问题来源。

### 1.1 MVP 范围

本轮先交付三个模块的最小闭环，目标是验证边界是否成立，而不是一次做全。

**做：**

- 随手记：完成 `items` → `captures` 拆分，保留现有列表、详情、标签、搜索、仓库关联与导出。
- 待办：手动创建、列表、完成 / 取消、未完成计数。字段只有标题、备注、状态、来源。
- 收件箱：消息列表、已读、忽略、转为待办、未判断计数。先不接真实连接器，用开发用 ingest 命令和 fixtures 验证判断流程。

**不做（后续阶段）：**

- GitHub 通知与邮件连接器
- 从收件箱提取为随手记
- 待办截止时间、优先级、标签、全文搜索、子任务
- 消息源管理界面、保留策略、批量清理
- 全局搜索覆盖待办或收件箱

MVP 的完成标准是：三个领域各自可用、边界不互相污染；连接器可以在不改表结构的前提下接上。

## 2. 现状诊断

### 2.1 一张表承担了三种语义

`items` 目前同时表示：

- 快速收集的笔记与链接（`source = clipboard`）
- 仓库详情里挂靠的笔记（`source = repo`，带 `repo_id`）
- 早期的 GitHub 事件（`kind = github`，已在 `0003_repo_workspace.sql` 中清空，但 CHECK 仍保留该值）

前两种是用户资产，第三种已经迁到 `github_repos` / `github_threads` / `github_releases`。表结构没有随领域拆分而收敛。

### 2.2 `status` 把「判断」和「执行」混在一起

`ItemStatus` 为 `pending / processed / archived`，页面按「待处理 / 已处理 / 已归档」分栏，侧边栏角标显示 `pending` 数量。

这套状态同时承担了两件事：

- 收件箱的**判断**：这条消息看过了吗、要不要处理？
- 待办的**执行**：这件事做完了吗？

结果是两边都不准确：收集一篇文章并不存在「处理完成」，而一条 GitHub 通知转为待办后，又需要另一套完成状态。判断和执行必须分开建模。

### 2.3 `note` 与 `body` 语义混乱

`application/capture.rs` 保存纯文本时把内容写进 `note`，而链接的补充说明也写进 `note`；`body` 只用于链接回源抓取的正文。结果是：

- 笔记类条目的正文在 `note`，链接类条目的正文在 `body`；
- 列表摘要、详情正文、FTS 索引三处都要做兼容判断；
- 导出时无法稳定区分「正文」与「补充说明」。

### 2.4 `inbox` 这个名字被占用了

`src/features/inbox/Inbox.tsx` 实际是「个人收集」页面，组件名、目录名与产品语义不一致。真正的收件箱（外部消息）还没有位置。

### 2.5 GitHub 已有独立模块，收件箱不能重复建设

`repos` 模块已经承载仓库、PR、Issue、Release 的同步与详情。收件箱里的 GitHub 消息应定位为**通知事件**（mention、review requested、assigned、CI 失败等），只存事件摘要与跳转引用，不复制 PR 正文、评论和 Release 内容。否则同一份数据会有两套缓存和两套刷新逻辑。

## 3. 职责边界

| 维度       | 收件箱 Inbox                                | 待办 Task                      | 随手记 Capture               |
| ---------- | ------------------------------------------- | ------------------------------ | ---------------------------- |
| 数据来源   | 外部系统推送或拉取                          | 收件箱转入、手动创建           | 用户输入、剪贴板、仓库笔记   |
| 写入方     | 连接器（系统）                              | 用户                           | 用户                         |
| 内容可变性 | 原文不可编辑，只改已读 / 判断结果           | 可编辑标题、备注               | 可编辑标题、正文、备注、标签 |
| 生命周期   | 短期停留，判断后离开                        | 有限生命周期，完成或取消       | 长期沉淀，可归档但默认保留   |
| 状态模型   | `read_at` + `triaged_at` / `triage_outcome` | `open / done / cancelled`      | `archived_at` 为空即未归档   |
| 去重       | `(source_id, external_id)` 唯一             | 同一来源只生成一个待办         | 不去重，同一内容可记多条     |
| 检索       | 按来源、类型、时间、未判断筛选              | 按状态、时间筛选，后续可加全文 | FTS5 全文检索                |
| 标签       | 不支持                                      | 不支持                         | 支持                         |
| 界面       | 消息流 + 未判断计数 + 判断动作              | 清单 + 完成 / 取消             | 列表 + 详情 + 标签           |
| 出口       | 忽略、转待办、提取为随手记                  | 完成、取消                     | 挂到仓库，未来可转待办       |

### 3.1 三个领域的流转

```text
外部系统 ──连接器──▶ 收件箱 ──判断──┬── 忽略（dismissed）
                                    ├── 转为待办 ──▶ 待办 ──▶ 完成 / 取消
                                    └── 提取为随手记 ──▶ 随手记（可选）

手动创建 ───────────────────────────▶ 待办
手动输入 / 剪贴板 ───────────────────▶ 随手记
```

方向是单向的：

- 收件箱不会自动流入待办或随手记，必须人工判断。
- 待办完成后不会变成随手记；随手记也不会自动变成待办。
- 随手记不会反向流入收件箱。

### 3.2 收件箱的三种判断结果

一条消息进入收件箱后，只有三种归宿：

1. **忽略**：`triage_outcome = dismissed`。消息从待判断列表消失，但底层保留，用于去重和审计。
2. **转为待办**：`triage_outcome = task`，`triage_ref_id` 指向新建的 `tasks.id`。这是 GitHub 通知的主要归宿。
3. **提取为随手记**：`triage_outcome = capture`，`triage_ref_id` 指向新建的 `captures.id`。只在消息里包含值得长期保留的资源时使用，是可选动作。

MVP 只实现前两种；提取为随手记留到后续阶段。

「忽略」不做硬删除，原因有两个：

- 硬删除后，下一次同步会重新插入同一条消息，用户会反复看到已经忽略的内容；
- 保留软删除记录可以做审计和批量清理，成本远低于维护一张墓碑表。

底层保留恢复能力，MVP 不暴露「显示已忽略」入口，避免增加筛选复杂度；真正的物理清理交给后续的保留策略任务。

「已读」和「已判断」是两件事：`read_at` 只表示看过，`triaged_at` 表示已经做出判断。用户可以读完一条消息但暂时不判断，它仍然留在待判断列表里。

### 3.3 随手记与待办的边界

- **随手记是参考资料，没有完成状态。** 一篇文章、一个想法、一段摘录，价值在于以后能查到，不在于「做完」。
- **待办是行动，必须有完成状态。** 一件事要么完成，要么取消，不能无限停留在清单里。
- **不要把待办塞回随手记。** 给 `captures` 加 `status` 会重新制造当前的问题；待办单独建表。
- **待办只存行动所需的短文本。** 长正文、链接、摘录留在随手记或收件箱里，待办通过 `origin_kind` / `origin_id` 指回来源。
- 未来可以支持「从随手记创建待办」，但这是显式动作，不是自动流转。

### 3.4 与仓库模块的边界

- `repos`：我关注的仓库及其 PR / Issue / Release 的**完整内容**，按仓库组织。
- `inbox`：与我有关的**通知事件**，按时间组织，只保留摘要和引用。
- GitHub 通知命中本地已同步的线程时，写入 `related_kind = github_thread`、`related_id = 本地线程 id`，点击直接进详情；未同步时只存 URL，点击走外部浏览器。
- 收件箱不同步 PR 正文与评论，避免与 `github_threads` / `github_thread_comments` 重复。
- 转为待办时，待办标题取通知标题，备注取摘要与 URL；线程内容仍然回到 `repos` 查看。

## 4. 模块拆分

### 4.1 命名

| 概念         | 代码名                    | 说明                                   |
| ------------ | ------------------------- | -------------------------------------- |
| 一条随手记   | `Capture`                 | 替代 `Item`                            |
| 随手记列表页 | `features/captures/`      | 替代 `features/inbox/Inbox.tsx`        |
| 快速收集窗口 | `features/quick-capture/` | 原 `features/capture/`，只负责输入窗口 |
| 一条待办     | `Task`                    | 新模块                                 |
| 收件箱       | `Inbox` / `InboxMessage`  | 新模块，外部消息                       |
| 消息源       | `InboxSource`             | GitHub、邮件等连接器实例               |

`capture` 表示「一次捕获」，`captures` 表示「随手记集合」，`quick-capture` 表示入口窗口。三者分开后，目录名不再有歧义。

### 4.2 后端目录

```text
src-tauri/src/
├── domain/
│   ├── capture.rs          # Capture / CaptureKind / CaptureSummary / CapturePage / CaptureCounts
│   ├── task.rs             # Task / TaskStatus / TaskSummary / TaskPage / TaskCounts
│   └── inbox.rs            # InboxSource / InboxMessage / InboxPage / InboxCounts
├── application/
│   ├── captures.rs         # 链接识别、网页元数据抓取、创建随手记
│   ├── tasks.rs            # 待办创建、完成、取消
│   └── inbox/
│       ├── mod.rs          # 消息入库、去重、判断动作
│       ├── github.rs       # GitHub 通知连接器（阶段四）
│       └── email.rs        # 邮件连接器（阶段五）
├── commands/
│   ├── captures.rs         # create / get / list / count / update / archive / search
│   ├── quick_capture.rs    # 收集窗口：capture_from_clipboard / open / dismiss
│   ├── tasks.rs            # create / list / count / update / complete / cancel
│   └── inbox.rs            # list / count_pending / mark_read / dismiss / to_task / to_capture
└── infrastructure/db/repositories/
    ├── capture.rs
    ├── task.rs
    ├── inbox_source.rs
    └── inbox_message.rs
```

`domain/mod.rs` 只导出 `capture::*`、`task::*`、`inbox::*`，不再导出 `Item*`。

### 4.3 前端目录

```text
src/features/
├── captures/
│   ├── CapturesPage.tsx    # 原 Inbox.tsx：列表 + 筛选 + 详情
│   └── CaptureDetail.tsx   # 从原文件拆出详情面板
├── quick-capture/
│   └── QuickCapture.tsx    # 原 features/capture/QuickCapture.tsx
├── tasks/
│   ├── TasksPage.tsx       # 新：待办清单
│   └── TaskDetail.tsx      # 可选：侧栏详情
└── inbox/
    ├── InboxPage.tsx       # 新：消息流 + 未判断筛选 + 判断动作
    └── InboxMessageDetail.tsx
```

`src/lib/api.ts` 建议同步拆成 `src/lib/api/captures.ts`、`tasks.ts`、`inbox.ts`、`github.ts`、`app.ts`，由 `index.ts` 统一再导出。当前单文件已超过 500 行，继续叠加会让类型与调用点互相干扰。

### 4.4 IPC 命令命名

| 旧命令                  | 新命令                                     |
| ----------------------- | ------------------------------------------ |
| `create_item`           | `create_capture`                           |
| `get_item`              | `get_capture`                              |
| `get_item_body`         | `get_capture_body`                         |
| `list_items_page`       | `list_captures_page`                       |
| `count_items_by_status` | `count_captures`                           |
| `update_item`           | `update_capture`                           |
| `archive_item`          | `archive_capture`                          |
| `search_items`          | `search_captures`                          |
| `set_item_tags`         | `set_capture_tags`                         |
| `get_item_tags`         | `get_capture_tags`                         |
| `capture_item`          | `capture_from_clipboard`                   |
| —                       | `restore_capture`                          |
| —                       | `create_task`                              |
| —                       | `list_tasks_page`                          |
| —                       | `count_tasks`                              |
| —                       | `update_task`                              |
| —                       | `complete_task`                            |
| —                       | `cancel_task`                              |
| —                       | `reopen_task`                              |
| —                       | `list_inbox_messages_page`                 |
| —                       | `count_inbox_pending`                      |
| —                       | `mark_inbox_message_read`                  |
| —                       | `mark_inbox_message_unread`                |
| —                       | `dismiss_inbox_message`                    |
| —                       | `convert_inbox_message_to_task`            |
| —                       | `convert_inbox_message_to_capture`（可选） |
| —                       | `ingest_inbox_message`（仅开发与测试）     |

前后端随安装包一起发布，不需要保留旧命令别名。一次性改完，避免长期双轨。

## 5. 数据模型

### 5.1 `captures`

```sql
CREATE TABLE captures (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('note', 'link')),
    -- quick_capture / repo / manual / import / inbox
    source TEXT NOT NULL,
    url TEXT,
    title TEXT,
    -- 列表用短摘要，最多 200 字
    summary TEXT,
    -- 完整正文：笔记原文或链接抓取结果
    body TEXT,
    -- 补充说明：链接的备注；笔记类通常为空
    note TEXT,
    -- 挂靠仓库时指向 github_repos，普通随手记为 NULL
    repo_id TEXT REFERENCES github_repos(id) ON DELETE SET NULL,
    -- 归档时间；NULL 表示未归档
    archived_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX captures_updated_at_id
    ON captures(updated_at DESC, id DESC);
CREATE INDEX captures_active_updated_at_id
    ON captures(updated_at DESC, id DESC) WHERE archived_at IS NULL;
CREATE INDEX captures_kind_updated_at_id
    ON captures(kind, updated_at DESC, id DESC);
CREATE INDEX captures_repo_updated_at_id
    ON captures(repo_id, updated_at DESC, id DESC);
```

关键变化：

- 删除 `status` 枚举，改为 `archived_at`。归档是整理动作，不是工作流状态。
- `kind` 只保留 `note` / `link`。`github` 已无数据，但 SQLite 无法直接修改 CHECK；沿用 0003 的做法，把该取值保留为已知技术债，不为此重建整表。
- `body` 与 `note` 的语义固定下来：`body` 是正文，`note` 是补充说明。
- `repo_id` 暂时保留，仓库详情「相关随手记」继续可用。

`CaptureCounts` 从四字段改为三字段：

```rust
pub struct CaptureCounts {
    pub all: i64,
    pub active: i64,
    pub archived: i64,
}
```

### 5.2 `tasks`

```sql
CREATE TABLE tasks (
    id TEXT PRIMARY KEY NOT NULL,
    title TEXT NOT NULL,
    note TEXT,
    status TEXT NOT NULL DEFAULT 'open'
        CHECK (status IN ('open', 'done', 'cancelled')),
    -- 来源：manual / inbox / capture
    origin_kind TEXT,
    origin_id TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    completed_at TEXT
);

CREATE INDEX tasks_status_updated_at
    ON tasks(status, updated_at DESC, id DESC);
-- 同一条收件箱消息只能生成一个待办，避免重复转换。
-- 只约束 inbox 来源；随手记未来可以生成多个待办。
CREATE UNIQUE INDEX tasks_inbox_origin_unique
    ON tasks(origin_id)
    WHERE origin_kind = 'inbox';
```

设计要点：

- 状态只有 `open / done / cancelled`，不引入 `doing`，避免又多一套工作流。
- `origin_kind` / `origin_id` 是弱关联，不建外键。来源可能是收件箱消息、随手记，未来也可能是仓库线程；强外键会让删除来源时连带破坏待办。
- 唯一索引保证「一条消息只转一个待办」。如果待办被取消后需要重新处理，恢复该待办即可，不新建第二条。
- MVP 不建 `due_at` / `priority`。需要时用 `ALTER TABLE tasks ADD COLUMN` 补，不提前留空字段。
- `note` 只放行动所需的短说明；长正文、链接、摘录留在随手记或收件箱，待办通过来源字段指回去。

`TaskCounts`：

```rust
pub struct TaskCounts {
    pub open: i64,
    pub done: i64,
    pub cancelled: i64,
    pub all: i64,
}
```

### 5.3 `inbox_sources`

```sql
CREATE TABLE inbox_sources (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('github', 'email', 'rss', 'manual')),
    name TEXT NOT NULL,
    -- 非敏感配置：仓库筛选、邮箱地址、同步间隔等
    config_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(config_json)),
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    -- 增量游标：GitHub 用 last_read_at / ETag，邮件用 UIDVALIDITY + UID
    cursor TEXT,
    last_synced_at TEXT,
    last_error TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
```

凭据不写入 `config_json`。GitHub Token 继续走 `infrastructure/secrets`（Windows Credential Manager），键名按 `inbox_source:<id>` 区分；邮件授权码同理。

`manual` 来源只用于开发用 ingest 与未来的手动转发，MVP 不暴露消息源管理界面。

### 5.4 `inbox_messages`

```sql
CREATE TABLE inbox_messages (
    id TEXT PRIMARY KEY NOT NULL,
    source_id TEXT NOT NULL REFERENCES inbox_sources(id) ON DELETE CASCADE,
    -- 来源侧唯一 id：GitHub notification id、邮件 Message-ID
    external_id TEXT NOT NULL,
    -- 来源侧更新时间，用于判断是否需要重新提醒
    external_updated_at TEXT,
    -- mention / review_requested / assigned / ci_failed / release / mail
    category TEXT NOT NULL,
    title TEXT NOT NULL,
    summary TEXT,
    body TEXT,
    url TEXT,
    -- 触发人 / 发件人
    actor TEXT,
    occurred_at TEXT NOT NULL,
    received_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    -- 看过不等于判断过
    read_at TEXT,
    -- 判断时间与结果
    triaged_at TEXT,
    triage_outcome TEXT CHECK (triage_outcome IN ('dismissed', 'task', 'capture')),
    -- tasks.id 或 captures.id，弱关联
    triage_ref_id TEXT,
    -- 关联本地实体：github_repo / github_thread / github_release
    related_kind TEXT,
    related_id TEXT,
    payload_json TEXT CHECK (payload_json IS NULL OR json_valid(payload_json)),
    UNIQUE(source_id, external_id),
    CHECK (
        (triaged_at IS NULL AND triage_outcome IS NULL AND triage_ref_id IS NULL)
        OR (triaged_at IS NOT NULL AND triage_outcome = 'dismissed' AND triage_ref_id IS NULL)
        OR (triaged_at IS NOT NULL AND triage_outcome IN ('task', 'capture') AND triage_ref_id IS NOT NULL)
    )
);

CREATE INDEX inbox_messages_pending
    ON inbox_messages(occurred_at DESC, id DESC)
    WHERE triaged_at IS NULL;
CREATE INDEX inbox_messages_unread
    ON inbox_messages(occurred_at DESC, id DESC)
    WHERE read_at IS NULL AND triaged_at IS NULL;
CREATE INDEX inbox_messages_source
    ON inbox_messages(source_id, occurred_at DESC, id DESC);
```

设计要点：

- 不设 `status` 枚举。未读、未判断、已判断全部由时间字段推导，避免三处状态互相矛盾。
- `read_at` 与 `triaged_at` 分离：可以读完先放着，也可以直接判断。
- `triage_outcome` 只有三种，对应收件箱的三个出口。`capture` 取值预留给后续的提取动作，MVP 只使用 `dismissed` 与 `task`；保留它是因为 SQLite 修改 CHECK 需要重建表。
- `triage_ref_id` 弱关联，不建外键。消息可能先于待办或随手记存在，强外键会限制写入顺序；跳转时再校验目标是否存在。
- `external_id` 保证重复同步不产生重复消息；`external_updated_at` 用于判断「同一条通知有了新进展」。
- `payload_json` 保留原始载荷，便于排查连接器问题；界面不直接渲染它。

### 5.5 判断动作的事务

**转为待办**：

```sql
BEGIN;
INSERT INTO tasks(id, title, note, origin_kind, origin_id)
VALUES (?, ?, ?, 'inbox', ?);
UPDATE inbox_messages
SET triaged_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
    triage_outcome = 'task',
    triage_ref_id = ?
WHERE id = ? AND triaged_at IS NULL;
COMMIT;
```

`WHERE triaged_at IS NULL` 保证不会重复判断；如果更新影响 0 行，回滚整个事务。`tasks_origin_unique` 作为第二道防线。

**忽略**：

```sql
UPDATE inbox_messages
SET triaged_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
    triage_outcome = 'dismissed',
    triage_ref_id = NULL
WHERE id = ? AND triaged_at IS NULL;
```

**提取为随手记**（可选）：与转为待办同构，先插入 `captures`，再把 `triage_outcome` 设为 `capture`、`triage_ref_id` 设为新 id。只取 URL / 标题 / 摘要，没有 URL 时才取正文，不镜像整条消息。

### 5.6 去重与更新

```sql
INSERT INTO inbox_messages (...) VALUES (...)
ON CONFLICT(source_id, external_id) DO UPDATE SET
    title = excluded.title,
    summary = excluded.summary,
    body = excluded.body,
    url = excluded.url,
    actor = excluded.actor,
    occurred_at = excluded.occurred_at,
    external_updated_at = excluded.external_updated_at,
    read_at = CASE
        WHEN inbox_messages.triaged_at IS NOT NULL
            THEN inbox_messages.read_at
        WHEN excluded.external_updated_at > COALESCE(inbox_messages.external_updated_at, '')
            THEN NULL
        ELSE inbox_messages.read_at
    END;
```

规则：

- 已判断的消息不因重新同步而恢复未读，也不清除判断结果。
- 未判断的消息如果有新进展，重新变为未读，提醒用户再看一眼。
- `triaged_at` / `triage_outcome` / `triage_ref_id` 永远不被同步覆盖。

### 5.7 检索

- 随手记：FTS5 覆盖 `title / summary / note / body`。
- 待办：阶段二先按状态和时间筛选，不做全文；待办数量通常不大，等出现真实需求再加 FTS。
- 收件箱：按来源、类型、时间、未判断筛选，不做全文；消息是短期数据，全文检索的收益低。

全局搜索仍然只覆盖随手记。待办和收件箱各自有页面内筛选，不混进同一个结果列表，避免搜索语义再次模糊。

## 6. 迁移方案

拆成三个迁移，分别对应三个领域，便于独立验证和回滚。

### 6.1 `0005_capture_module.sql`

步骤：

1. 删除 `items_fts` 的三个触发器和虚拟表。
2. 删除引用 `status` 的索引：`items_status_created_at`、`items_status_updated_at_id`。
3. `ALTER TABLE items RENAME TO captures;`
4. `ALTER TABLE item_tags RENAME TO capture_tags;`
5. 新增 `archived_at`，按旧状态回填。
6. 笔记类把 `note` 搬进 `body`，并清空 `note`。
7. 删除 `status` 列。
8. 重建索引，索引名统一为 `captures_*`。
9. 重建 `captures_fts` 与触发器，全量回填。
10. 归一化 `source`：`clipboard` → `quick_capture`，`repo` 保持不变。

数据映射：

| 旧值                         | 新值                          |
| ---------------------------- | ----------------------------- |
| `kind = note`，`note = 正文` | `body = note`，`note = NULL`  |
| `kind = link`，`note = 备注` | `note` 保留                   |
| `status = pending`           | `archived_at = NULL`          |
| `status = processed`         | `archived_at = NULL`          |
| `status = archived`          | `archived_at = updated_at`    |
| `source = clipboard`         | `source = quick_capture`      |
| `repo_id`                    | 原样保留                      |
| `kind = github`              | 0003 已无数据；CHECK 暂不收紧 |

`processed` 与 `pending` 都归为未归档。这是有意的语义删除：拆分后不再有「处理完成」状态，原「已处理」内容仍应留在随手记里可见。迁移提示需要明确告知这一点。

### 6.2 `0006_tasks.sql`

只建 `tasks` 表与索引，不写入数据。阶段二可以先上线手动待办，不依赖收件箱。

### 6.3 `0007_inbox_module.sql`

只建 `inbox_sources` 与 `inbox_messages` 及索引，不写入数据。收件箱初始为空，由连接器后续填充。

### 6.4 迁移提示

当前 `MigrationNotice` 只有 `migratedTo` 和 `backupPath`，前端 `AppShell.tsx` 里硬编码了「旧 GitHub 条目已清理」的文案。0005 之后这句话不再成立。

建议后端在迁移时直接返回本次迁移的说明：

```rust
pub struct MigrationNotice {
    pub migrated_to: Option<i64>,
    pub backup_path: Option<String>,
    pub message: Option<String>,
}
```

由 `database.rs` 按迁移版本生成文案，前端只负责展示。这样以后新增迁移不需要再改前端。

### 6.5 回滚

- 迁移前会自动备份到 `<数据目录>/backup/tomatodesk-v4-<时间戳>.sqlite3`（`database.rs` 现有逻辑）。
- 回滚方式：退出应用 → 用备份覆盖 `tomatodesk.sqlite3` → 重装 v0.4.x。
- 降级不支持原地兼容：v0.4 的可执行文件只认 `items` 表。文档和发布说明里要写清楚「降级必须恢复备份」。
- `0006` / `0007` 只新增表，回滚时直接删除新表即可；但如果已经产生待办或收件箱数据，仍然建议恢复备份。

## 7. 影响面清单

### 7.1 后端

| 文件                                              | 变化                                                                             |
| ------------------------------------------------- | -------------------------------------------------------------------------------- |
| `domain/item.rs`                                  | 改为 `domain/capture.rs`，删除 `ItemStatus`                                      |
| `domain/mod.rs`                                   | 导出改为 `capture` / `task` / `inbox`                                            |
| `infrastructure/db/repositories/item.rs`          | 改为 `capture.rs`，SQL 表名与状态条件调整                                        |
| `infrastructure/db/repositories/mod.rs`           | 新增 `task` / `inbox_source` / `inbox_message`                                   |
| `infrastructure/db/database.rs`                   | 注册 0005 / 0006 / 0007，新增 `captures()` / `tasks()` / `inbox()`，扩展迁移提示 |
| `commands/items.rs`                               | 改为 `commands/captures.rs`                                                      |
| `commands/capture.rs`                             | 改为 `commands/quick_capture.rs`                                                 |
| `commands/tasks.rs`                               | 新增                                                                             |
| `commands/inbox.rs`                               | 新增                                                                             |
| `commands/dto.rs`                                 | 按 captures / tasks / inbox 拆分，删除 `ItemStatus` 相关入参                     |
| `commands/tags.rs`                                | `set_item_tags` / `get_item_tags` 改名，校验目标表改为 `captures`                |
| `application/capture.rs`                          | 改为 `application/captures.rs`，笔记正文写入 `body`                              |
| `application/tasks.rs`                            | 新增                                                                             |
| `application/inbox/`                              | 新增                                                                             |
| `application/export.rs`                           | 导出结构改为 `captures`，`formatVersion` 升到 2                                  |
| `commands/tests.rs`、`infrastructure/db/tests.rs` | 全量跟随重命名，补迁移、待办、inbox 去重测试                                     |
| `lib.rs`                                          | 更新 `generate_handler!` 列表                                                    |

### 7.2 前端

| 文件                                 | 变化                                                                                  |
| ------------------------------------ | ------------------------------------------------------------------------------------- |
| `features/inbox/Inbox.tsx`           | 移到 `features/captures/CapturesPage.tsx`，去掉状态分栏                               |
| `features/capture/QuickCapture.tsx`  | 移到 `features/quick-capture/`，保存文案改为「保存到随手记」                          |
| `features/tasks/TasksPage.tsx`       | 新建，待办清单与完成 / 取消                                                           |
| `features/inbox/InboxPage.tsx`       | 新建，消息流 + 未判断筛选 + 判断动作                                                  |
| `app/AppShell.tsx`                   | 导航改为「收件箱 / 待办 / 随手记 / 仓库 / 搜索 / 设置」，角标分别显示未判断与未完成数 |
| `lib/api.ts`                         | 拆分为 captures / tasks / inbox / github / app                                        |
| `lib/events.ts`                      | `items-changed` 改为 `captures-changed`，新增 `tasks-changed` / `inbox-changed`       |
| `features/search/SearchPage.tsx`     | 调用 `search_captures`，类型改为 `CaptureSummary`                                     |
| `features/repos/RepoDetail.tsx`      | 「我的条目」改为「相关随手记」，状态按钮改为归档开关                                  |
| `features/settings/SettingsPage.tsx` | 预留消息源管理入口（阶段三只读展示）                                                  |
| `features/dev/fixtures.ts`           | 类型与假数据跟随调整                                                                  |

### 7.3 文档

- 更新 `docs/execution-plans/tomatodesk-mvp.md` 中的 Inbox 定义，避免后续按旧语义继续开发。
- 发布说明写清楚状态模型变化与降级需要恢复备份。
- `README.md` 按项目约定默认不改；如需同步「收件箱 / 待办」描述，由用户确认后再动。

## 8. 分阶段实施

### 阶段一：随手记独立（0005）

范围：数据库迁移、后端重命名、前端页面迁移、搜索 / 标签 / 仓库 / 导出跟随调整。

验收：

- 旧库迁移后条目数量与迁移前一致，标签关联不丢。
- 笔记正文出现在详情「正文」区，链接备注仍在「备注」区。
- 归档 / 取消归档可用，列表默认只显示未归档。
- FTS 能搜到旧条目的正文。
- 仓库详情「相关随手记」可新增、可归档。
- 导出 JSON / Markdown 内容完整。

落地记录（2026-11-26）：

- `0005_capture_module.sql`：`items` → `captures`、`status` → `archived_at`、笔记类正文并入 `body`、`items_fts` 重建并回填、`item_tags` 重建为 `capture_tags`（列名同步改为 `capture_id`）、`source` 归一化 `clipboard` → `quick_capture`。
- 后端：`domain/capture.rs`、`repositories/capture.rs`、`commands/captures.rs`、`commands/quick_capture.rs`、`application/captures.rs`；`github_repo` 计数列改为 `captures`；`ExportBundle` 改为 `captures` 且 `format_version` 升到 2。
- 前端：`features/captures/{CapturesPage,CaptureDetail,shared}`、`features/quick-capture/QuickCapture.tsx`；`AppShell` 导航改为「随手记 / 仓库 / 搜索 / 设置」，角标显示未归档数，迁移提示改读 `notice.message`。
- 与方案的偏差：`CaptureDetail.tsx` 按 4.3 一并拆出；`features/capture/` 目录整体改名为 `features/quick-capture/`；仓库详情的「当前待办」小节改名为「进行中」，避免与阶段二的待办模块撞名。
- 门禁：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --locked`（30/30）、`pnpm test`、`pnpm lint`、`pnpm build`、`pnpm check:design`、`pnpm format:check` 全部通过。
- 待人工复核：真实数据库副本的迁移结果、`?preview` 下逐页视觉走查（本轮未截图）。

### 阶段二：待办骨架（0006）

范围：`tasks` 表、手动创建、列表、完成 / 取消、计数。先不接收件箱。MVP 字段只有标题、备注、状态、来源；不做截止时间、优先级、标签、搜索、子任务。

验收：

- 可以手动创建待办，编辑标题与备注。
- 完成 / 取消可逆，列表按状态筛选。
- 导航「待办」角标显示未完成数量。
- 待办与随手记互不干扰：待办没有标签和 FTS，随手记没有完成状态。

落地记录（2026-11-26）：

- 迁移：`0006_tasks.sql` 只建 `tasks` 表与 `tasks_status_updated_at`、`tasks_inbox_origin_unique` 两个索引，不写入数据；`database.rs` 注册为第 6 个迁移，无用户可见提示。
- 后端：`domain/task.rs`（`TaskStatus` / `Task` / `TaskPage` / `TaskCounts` / `NewTask`）、`repositories/task.rs`、`application/tasks.rs`、`commands/tasks.rs`；`lib.rs` 注册 `create_task`、`list_tasks_page`、`count_tasks`、`update_task`、`complete_task`、`cancel_task`、`reopen_task` 七个命令。
- 状态语义：`open / done / cancelled` 三态；完成时写 `completed_at`（重复完成保留首次时间），取消与重开清空；`origin_kind = inbox` 受唯一索引约束，为阶段三的「转为待办」预留。
- 前端：`features/tasks/{TasksPage,TaskDetail,shared}`，列表与详情双栏、状态标签页、顶部快速新建、完成 / 取消 / 重开、标题与备注就地编辑；`lib/api.ts` 增加待办类型与调用；`lib/events.ts` 增加 `tasks-changed`；`AppShell` 导航加入「待办」并显示未完成角标，默认页仍是随手记。
- 与方案的偏差：列表直接返回 `Task`，没有单列 `TaskSummary`（待办字段都是短文本，拆类型没有收益）；未提供 `get_task`（列表行已含全部字段，详情不需要二次请求）；导航暂不出现「收件箱」入口，留给阶段三。
- 门禁：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --locked`（34/34）；`pnpm test`、`pnpm lint`、`pnpm build`、`pnpm check:design`、`pnpm format:check` 全部通过。
- 走查：`?preview&view=tasks` 在 1440×900 与 820×900 下截图核对，列表、详情、角标与空状态布局正常。
- 待人工复核：真实数据库升级到 v6 后的待办创建与状态流转、窗口内滚动与键盘操作。

### 阶段三：收件箱骨架（0007）

范围：`inbox_sources` / `inbox_messages` 表、消息列表、未判断计数、已读、忽略、转为待办、空状态页面、导航与角标。先不接真实连接器，用开发用 ingest 命令和 fixtures 造消息；不做提取为随手记、消息源管理界面、保留策略。

验收：

- 收件箱无消息时展示明确空状态，不出现报错。
- 角标显示未判断数，已读 / 忽略 / 转待办后立即更新。
- 「忽略」后消息从待判断列表消失，重复 ingest 不复活。
- 「转为待办」在同一个事务里创建待办并标记消息，重复点击不会生成两条待办。
- 没有连接器时不能触发同步，界面不暴露无效按钮。
- 开发用 ingest 命令只在测试与 `?preview` 中使用，不出现在正式界面。

落地记录（2026-11-26）：

- 迁移：`0007_inbox_module.sql` 只建 `inbox_sources` 与 `inbox_messages` 及两个 partial index（未判断、未读）与来源索引，不写入数据；`database.rs` 注册为第 7 个迁移，无用户可见提示。
- 后端：`domain/inbox.rs`（`InboxSource` / `InboxMessage` / `InboxPage` / `InboxPageQuery` / `InboxCounts` / `TriageOutcome` / `NewInboxMessage`）、`repositories/{inbox_source,inbox_message}.rs`、`application/inbox/mod.rs`、`commands/inbox.rs`；`lib.rs` 注册 `list_inbox_messages_page`、`count_inbox_pending`、`list_inbox_sources`、`mark_inbox_message_read`、`mark_inbox_message_unread`、`dismiss_inbox_message`、`convert_inbox_message_to_task`、`ingest_inbox_message`。
- 事务与幂等：`to_task` 在同一个事务里 `UPDATE ... WHERE triaged_at IS NULL` 后插入待办，影响 0 行即回滚；应用层再判一次，已转过的消息直接返回原待办，重复点击不产生第二条。忽略同样是 `WHERE triaged_at IS NULL`，重复点击幂等且不会改写已转待办的消息。
- 去重：`(source_id, external_id)` upsert 按方案 5.6 实现——已判断的消息不恢复未读、判断结果不被覆盖；未判断的消息有新进展时重新变为未读。补充：`related_kind` / `related_id` / `payload_json` 用 `COALESCE` 补写，空值不会清掉已有值，为阶段四的线程关联回填留口。
- 前端：`features/inbox/{InboxPage,InboxMessageDetail,shared}`——「待判断 / 已判断 / 全部」三栏、只看未读、列表与详情双栏、标记已读 / 未读、忽略、转为待办、打开原始链接、按筛选区分的空状态；`AppShell` 导航改为「收件箱 / 待办 / 随手记 / 仓库 / 搜索 / 设置」，收件箱角标显示未判断数，默认页仍是随手记；`lib/api.ts` 增加收件箱类型与调用，`lib/events.ts` 增加 `inbox-changed`（转待办时同时通知 `tasks-changed`），`fixtures.ts` 增加消息源、消息与计数。
- 与方案的偏差与补充：
  - 方案 5.4 的 `CHECK` 在 `triage_outcome` 为 NULL 时整条表达式求值为 NULL，SQLite 的 CHECK 只拒绝 false，会放行「只有 `triaged_at`」的半截状态；迁移里显式补 `triage_outcome IS NOT NULL` 并改用 `IS` 比较，让自洽约束真正生效（已加测试）。
  - `count_inbox_pending` 沿用方案命名，但一次返回 `InboxCounts { all, pending, unread }`，省一次 IPC；`InboxCounts` 的字段方案只给了类型名，此处定稿。
  - 新增 `list_inbox_sources`（方案 IPC 表未列），只供设置页只读展示，不含新增 / 删除 / 同步按钮，满足 7.2 的「阶段三只读展示」而不违反 8 的「不做消息源管理界面」。
  - `ingest_inbox_message` 注册进 handler 以便测试与开发窗口使用，但正式构建里直接返回错误；开发用 ingest 固定复用 `source-manual`（`kind = manual`），避免为它另建来源管理入口。
  - `lib/api.ts` 的拆分（4.3 的建议项）继续后移，本轮只在单文件里追加收件箱类型，避免与阶段三验收无关的大范围搬迁；已列入待办。
- 门禁：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --locked`（41/41）；`pnpm test`（6/6）、`pnpm lint`、`pnpm build`、`pnpm check:design`、`pnpm format:check` 全部通过。
- 待人工复核：真实数据库升级到 v7 后的收件箱空状态与角标、`?preview&view=inbox` 的逐页视觉走查（本轮未截图，环境无可用无头浏览器）、开发窗口里用 ingest 造 3 条消息走一遍「已读 / 忽略 / 转待办」。

接线补记（2026-11-26）：

- 后端接线已补齐：四个 `mod.rs` 的模块声明与导出、`database.rs` 的 0006 / 0007 注册与 `tasks()` / `inbox_sources()` / `inbox_messages()` 访问器、`commands/dto.rs` 的待办与收件箱入参、`lib.rs` 的 15 个 handler（7 待办 + 8 收件箱）。
- 门禁：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test` 37/37；新增用例覆盖待办状态流转与来源唯一索引、收件箱去重与单次转待办、v5 库升级到 v7。
- 仍未落地：上面两段「落地记录」中的前端部分——`features/tasks`、`features/inbox`、`lib/api.ts` / `lib/events.ts` 调用、`AppShell` 导航与角标、`fixtures.ts` 的收件箱数据，以及 `?preview&view=tasks|inbox` 截图走查。阶段二、阶段三的验收标准在界面侧尚未满足。（已由下方「前端补记」补齐。）

前端补记（2026-11-26）：

- 待办页：`features/tasks/{TasksPage,TaskDetail,shared}`——状态标签页（进行中 / 已完成 / 已取消 / 全部）、顶部快速新建、列表与详情双栏、完成 / 取消 / 重开、标题与备注编辑、按筛选区分的空状态；分页用「加载更多」按钮，没有引入滚动哨兵。
- 收件箱页：`features/inbox/{InboxPage,InboxMessageDetail,shared}`——消息源栏 + 消息列表 + 详情三栏，窄屏下来源栏收起为筛选里的「全部来源」下拉；「待判断 / 已判断 / 全部」、只看未读、分类筛选、已读 / 未读、忽略、转为待办、空状态；开发窗口显示「造一条示例消息」，正式构建与 `?preview` 都不出现。
- 导航：`AppShell` 改为收件箱 / 待办 / 随手记 / 仓库 / 搜索 / 设置，角标分别显示未判断、未完成、未归档数，默认页仍是随手记。
- 事件：`lib/events.ts` 增加 `tasks-changed` 与 `inbox-changed`；转待办时两个都发，待办角标与收件箱角标同步更新。
- 走查数据：`fixtures.ts` 的待办与收件箱是可变数组，`?preview` 下新建、完成、重开、已读、忽略、转待办都能走完整个闭环，不需要真实 IPC。
- 偏差：详情编辑是「编辑 → 保存」，不是每个字段失焦即存；「只看未读」下打开消息不自动标已读，避免一边扫读一边从列表消失；初始自动选中第一条也不算已读，只有点开才算。
- 门禁：`pnpm exec tsc --noEmit`、`pnpm lint`、`pnpm build`、`pnpm test`（6/6）、`pnpm check:design`、`pnpm format:check` 全部通过。
- 走查：无头 Chrome 在 1440×900 与 820×900 下逐页截图；再用 CDP 实际点击走通「打开消息即已读 → 忽略 → 转待办 → 去待办页看到新条目」「完成 → 已完成标签页 → 重开」「新建待办」三条路径，无控制台报错。
- 待人工复核：真实数据库（v7）下用窗口内的 ingest 造 3 条消息，确认角标与列表刷新；窄屏下的滚动与键盘操作。

### 阶段四：GitHub 通知连接器

范围：`GET /notifications` 拉取、`(source_id, external_id)` 去重、线程关联、已读回写。

验收：

- 重复同步不产生重复消息。
- 通知有新进展时重新变为未读，已判断的不复活。
- 命中本地线程时跳详情，否则打开 GitHub。
- 转为待办后，待办能显示「来自 GitHub 通知」并跳回原消息。
- Token 缺少 `notifications` scope 时有明确错误提示。

### 阶段五：邮件连接器（可延后）

范围：IMAP 增量拉取、Message-ID 去重、纯文本优先、HTML 清洗、发件人展示。

邮件涉及授权码存储、HTML 安全、编码与附件，建议单独排期，不与阶段四合并。

## 9. 验证方式

### 9.1 自动化

- 迁移测试：构造 v4 数据库，写入 pending / processed / archived 的笔记与链接、标签、`repo_id`，执行迁移后断言数量、`archived_at`、`body` / `note` 归属、FTS 命中、标签关联。
- Repository 测试：captures CRUD、分页、计数、搜索；tasks 创建、完成、取消、来源唯一；inbox 入库去重、已读保持、判断结果不被同步覆盖、未判断计数。
- 命令测试：参数校验、未知字段拒绝、归档幂等、重复转待办只生成一条。
- 常规门禁：`cargo test --locked`、`cargo clippy --all-targets -- -D warnings`、`pnpm test`、`pnpm lint`、`pnpm build`、`pnpm check:design`、`pnpm format:check`。

### 9.2 手工

- 复制一份真实数据库到隔离目录，用开发模式启动，逐项核对迁移结果。
- 快速收集一条链接和一段文本，确认分别落到 `link` / `note`，正文与备注位置正确。
- 搜索旧内容、给旧内容打标签、在仓库页新增随手记。
- 导出 JSON 与 Markdown，确认字段与数量。
- 手动创建待办，完成、取消、恢复。
- 用开发用 ingest 命令造 3 条消息，验证收件箱空状态、角标、已读 / 忽略 / 转待办交互。
- 确认正式界面没有同步按钮、消息源管理入口或「提取为随手记」入口。

### 9.3 性能

- 用 1 万条 captures 测试 FTS 重建耗时与首屏分页。
- 收件箱按 1 万条消息测试未判断计数与分页，确认 partial index 生效。
- 待办按 1 万条测试状态筛选与计数。

## 10. 风险与对策

| 风险                             | 影响                     | 对策                                                      |
| -------------------------------- | ------------------------ | --------------------------------------------------------- |
| FTS 外部内容表未随重命名更新     | 搜索静默失效             | 迁移中显式删除重建并回填；测试覆盖旧内容搜索              |
| `DROP COLUMN status` 被索引阻塞  | 迁移失败                 | 先删 `items_status_*` 索引，再删列                        |
| `processed` 语义丢失             | 用户感知状态变化         | 迁移前自动备份；迁移提示明确说明；`processed` 归为未归档  |
| 待办又被塞回随手记               | 重新制造状态混乱         | 独立 `tasks` 表；captures 不加 `status`                   |
| 收件箱判断状态与待办状态互相覆盖 | 两边状态不一致           | 消息只记判断结果，待办只记执行状态；转待办用事务          |
| 命令重命名前后端不同步           | 运行时报「命令不存在」   | 同一次提交完成前后端；CI 跑完整构建与测试                 |
| 收件箱与仓库重复存 PR 内容       | 数据双写、刷新不一致     | 收件箱只存通知摘要与 `related_id`，正文留在仓库模块       |
| 凭据写入 `config_json`           | 明文泄露                 | 凭据只进 Credential Manager，`config_json` 仅存非敏感配置 |
| 收件箱范围膨胀                   | 阶段三拖成完整邮件客户端 | 阶段门禁：先骨架，再接 GitHub，邮件单独排期               |

## 11. 完成标准

- 代码中不再出现 `Item` / `ItemStatus` / `items` 表相关命名（迁移 SQL 与历史文档除外）。
- 随手记页面没有「待处理 / 已处理」语义，只有归档与取消归档。
- 待办是独立模块，具备创建、完成、取消与未完成计数；MVP 不含截止时间、优先级、标签与搜索。
- 收件箱是独立模块，具备消息列表、未判断计数、已读、忽略、转为待办；MVP 用开发用 ingest 验证，不接真实连接器。
- 旧数据完整迁移，搜索、标签、仓库关联、导出均可用。
- 文档、迁移提示与发布说明对状态变化和降级方式有明确说明。
- MVP 不包含从收件箱提取随手记、真实连接器与保留策略；这些不阻塞 MVP 验收。

## 12. 评审要点

请重点确认以下几项，确认后再进入编码：

1. **MVP 范围**：是否同意先不接 GitHub / 邮件连接器，用开发用 ingest 命令验证收件箱判断流程。
2. **待办最小字段**：标题、备注、状态、来源是否够用；截止时间、优先级是否确认后移。
3. **收件箱双状态**：`read_at`（看过）与 `triaged_at`（判断过）是否需要同时保留。保留可以支持「读完先放着」，代价是多一个筛选维度。
4. **忽略语义**：忽略采用软删除（保留去重记录、可恢复），不做硬删除。是否接受。
5. **旧数据处理**：`pending` 与 `processed` 都归为未归档，`archived` 保留归档时间。是否接受「已处理」这个状态被删除。
6. **命名**：`Capture` / `Task` / `InboxMessage` 三个实体名是否确认，确认后全链路按此重命名。
