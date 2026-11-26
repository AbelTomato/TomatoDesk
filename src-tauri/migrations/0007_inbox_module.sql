-- 收件箱：外部事件与事务处理的入口，只存通知摘要与跳转引用，不复制 PR / Issue 正文。
-- 0007 只建表，不写入数据；消息由连接器（阶段四）或开发用 ingest 命令填充。

CREATE TABLE inbox_sources (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('github', 'email', 'rss', 'manual')),
    name TEXT NOT NULL,
    -- 非敏感配置：仓库筛选、邮箱地址、同步间隔等；凭据走 Credential Manager。
    config_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(config_json)),
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    -- 增量游标：GitHub 用 last_read_at / ETag，邮件用 UIDVALIDITY + UID。
    cursor TEXT,
    last_synced_at TEXT,
    last_error TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE inbox_messages (
    id TEXT PRIMARY KEY NOT NULL,
    source_id TEXT NOT NULL REFERENCES inbox_sources(id) ON DELETE CASCADE,
    -- 来源侧唯一 id：GitHub notification id、邮件 Message-ID。
    external_id TEXT NOT NULL,
    -- 来源侧更新时间，用于判断是否需要重新提醒。
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
    -- tasks.id 或 captures.id，弱关联，不建外键
    triage_ref_id TEXT,
    -- 关联本地实体：github_repo / github_thread / github_release
    related_kind TEXT,
    related_id TEXT,
    payload_json TEXT CHECK (payload_json IS NULL OR json_valid(payload_json)),
    UNIQUE(source_id, external_id),
    -- 判断结果与时间字段必须自洽。方案里的写法在 `triage_outcome` 为 NULL 时
    -- 整条表达式会求值为 NULL，而 SQLite 的 CHECK 只拒绝 false，NULL 会放行；
    -- 这里显式补 `triage_outcome IS NOT NULL`，让「只有 triaged_at」这种半截状态被拒。
    CHECK (
        (triaged_at IS NULL AND triage_outcome IS NULL AND triage_ref_id IS NULL)
        OR (
            triaged_at IS NOT NULL
            AND triage_outcome IS NOT NULL
            AND (
                (triage_outcome = 'dismissed' AND triage_ref_id IS NULL)
                OR (triage_outcome IN ('task', 'capture') AND triage_ref_id IS NOT NULL)
            )
        )
    )
);

-- 未判断列表与计数走 partial index，已判断的消息再多也不拖慢首屏。
CREATE INDEX inbox_messages_pending
    ON inbox_messages(occurred_at DESC, id DESC)
    WHERE triaged_at IS NULL;
CREATE INDEX inbox_messages_unread
    ON inbox_messages(occurred_at DESC, id DESC)
    WHERE read_at IS NULL AND triaged_at IS NULL;
CREATE INDEX inbox_messages_source
    ON inbox_messages(source_id, occurred_at DESC, id DESC);
