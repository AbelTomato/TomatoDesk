-- 待办独立：只存行动所需的短文本，不进入 captures，也不带标签与全文检索。
-- 0006 只建表，不写入数据；阶段二先上线手动待办，不依赖收件箱。

CREATE TABLE tasks (
    id TEXT PRIMARY KEY NOT NULL,
    title TEXT NOT NULL,
    note TEXT,
    -- 只有执行状态，不引入 doing，避免又多一套工作流。
    status TEXT NOT NULL DEFAULT 'open'
        CHECK (status IN ('open', 'done', 'cancelled')),
    -- 来源：manual / inbox / capture；弱关联，不建外键。
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
