-- 已读回写队列：本地已读 → GitHub 的 PATCH 失败时留一行，成功后立即删除。
-- 只在失败时才有行，正常情况下这张表是空的，所以不需要额外的清理任务。
-- 传输方向的「未读」不做回写，队列里只有「已读」一种意图。

CREATE TABLE inbox_read_back (
    id TEXT PRIMARY KEY NOT NULL,
    message_id TEXT NOT NULL REFERENCES inbox_messages(id) ON DELETE CASCADE,
    -- 冗余来源与外部 id：重试时不必回表读消息，消息被软删除后也能继续写。
    source_id TEXT NOT NULL REFERENCES inbox_sources(id) ON DELETE CASCADE,
    external_id TEXT NOT NULL,
    -- pending：等待下一轮；blocked：需要人工处理（重新授权或退避耗尽）。
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'blocked')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    next_attempt_at TEXT NOT NULL,
    last_error TEXT,
    -- transient：网络或 5xx；rate_limited：限流；auth：Token 无效或缺权限；exhausted：退避耗尽。
    last_error_kind TEXT CHECK (
        last_error_kind IS NULL
        OR last_error_kind IN ('transient', 'rate_limited', 'auth', 'exhausted')
    ),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    -- 同一个远端线程只需要一条待办，重复标已读只重置这一行。
    UNIQUE (source_id, external_id)
);

CREATE INDEX inbox_read_back_due ON inbox_read_back(status, next_attempt_at);
