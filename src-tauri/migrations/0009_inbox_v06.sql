-- v0.6 收件箱状态轴与后续阶段共用的底座（S2 落地，S4/S5 复用）。
-- 只加列、加表、加索引，不改已有列：旧版本程序按列名查询，读 v9 库不会报错。
-- 迁移前由 database.rs 自动备份（<data>/backup），因为 SQLite 无法撤销 ADD COLUMN。

-- 处理来源：NULL 是人处理的，'rule' 是规则自动处理的（S5 写入）。
ALTER TABLE inbox_messages ADD COLUMN triage_by TEXT
    CHECK (triage_by IS NULL OR triage_by = 'rule');

-- 「已转待办 / 已忽略」两个 Tab 按结果筛选，计数也走这条 partial index。
CREATE INDEX inbox_messages_outcome
    ON inbox_messages(triage_outcome, occurred_at DESC, id DESC)
    WHERE triaged_at IS NOT NULL;

-- 回写队列从「只有 GitHub 已读」扩成「按动作分派」（S4 使用）。
-- 存量行都是 GitHub 已读，默认值让它们继续按原逻辑处理。
ALTER TABLE inbox_read_back ADD COLUMN action TEXT NOT NULL DEFAULT 'github_read'
    CHECK (action IN ('github_read', 'email_seen'));
ALTER TABLE inbox_read_back ADD COLUMN uid INTEGER;
ALTER TABLE inbox_read_back ADD COLUMN uid_validity INTEGER;

-- 规则化自动处理（S5 使用）。条件之间是「且」；CHECK 拦住「空条件规则」，
-- 否则一条没有条件的规则会吞掉整个收件箱。
CREATE TABLE inbox_rules (
    id TEXT PRIMARY KEY NOT NULL,
    source_id TEXT REFERENCES inbox_sources(id) ON DELETE CASCADE,
    category TEXT,
    actor_contains TEXT,
    title_contains TEXT,
    action TEXT NOT NULL CHECK (action IN ('read', 'dismiss')),
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    hit_count INTEGER NOT NULL DEFAULT 0 CHECK (hit_count >= 0),
    last_hit_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (
        source_id IS NOT NULL
        OR category IS NOT NULL
        OR actor_contains IS NOT NULL
        OR title_contains IS NOT NULL
    )
);

CREATE INDEX inbox_rules_enabled ON inbox_rules(enabled, created_at, id);
