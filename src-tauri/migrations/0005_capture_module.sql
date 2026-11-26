-- 随手记独立：items 改名为 captures，去掉待办语义的 status。
-- 迁移前由 database.rs 自动备份数据库到 <数据目录>/backup/。

-- 1) 先拆掉 FTS 与引用 status 的索引，避免重命名和删列时被阻塞。
DROP TRIGGER IF EXISTS items_fts_insert;
DROP TRIGGER IF EXISTS items_fts_delete;
DROP TRIGGER IF EXISTS items_fts_update;
DROP TABLE IF EXISTS items_fts;

DROP INDEX IF EXISTS items_status_created_at;
DROP INDEX IF EXISTS items_updated_at_id;
DROP INDEX IF EXISTS items_status_updated_at_id;
DROP INDEX IF EXISTS items_kind_updated_at_id;
DROP INDEX IF EXISTS items_repo_updated_at_id;

-- 2) 表改名。item_tags 的外键会随 items 重命名一起指向 captures。
ALTER TABLE items RENAME TO captures;

-- item_tags 的列名仍是 item_id；重建为 capture_id，避免代码里留下旧语义。
CREATE TABLE capture_tags (
    capture_id TEXT NOT NULL REFERENCES captures(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (capture_id, tag_id)
);
INSERT INTO capture_tags(capture_id, tag_id)
SELECT item_id, tag_id FROM item_tags;
DROP TABLE item_tags;

-- 3) 归档改为时间字段；旧 archived 保留归档时间，pending / processed 都视为未归档。
ALTER TABLE captures ADD COLUMN archived_at TEXT;
UPDATE captures SET archived_at = updated_at WHERE status = 'archived';

-- 4) 笔记类的 note 实际是正文，搬进 body；链接类的 note 仍是补充说明。
UPDATE captures
SET body = COALESCE(NULLIF(trim(body), ''), note),
    note = NULL
WHERE kind = 'note';

-- 笔记类此前没有摘要，用正文补一条最多 200 字的列表摘要。
UPDATE captures
SET summary = CASE
        WHEN length(replace(replace(body, char(13), ' '), char(10), ' ')) > 200
            THEN substr(replace(replace(body, char(13), ' '), char(10), ' '), 1, 200) || '…'
        ELSE replace(replace(body, char(13), ' '), char(10), ' ')
    END
WHERE kind = 'note'
  AND (summary IS NULL OR trim(summary) = '')
  AND body IS NOT NULL
  AND trim(body) <> '';

-- 5) 统一来源命名，旧值不再使用。
UPDATE captures SET source = 'quick_capture' WHERE source = 'clipboard';

-- 6) 删除 status 列。CHECK 中 kind 的 'github' 取值保留为已知技术债，不重建整表。
ALTER TABLE captures DROP COLUMN status;

-- 7) 重建索引：默认列表只查未归档，用部分索引减小体积。
CREATE INDEX IF NOT EXISTS captures_updated_at_id
    ON captures(updated_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS captures_active_updated_at_id
    ON captures(updated_at DESC, id DESC) WHERE archived_at IS NULL;
CREATE INDEX IF NOT EXISTS captures_kind_updated_at_id
    ON captures(kind, updated_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS captures_repo_updated_at_id
    ON captures(repo_id, updated_at DESC, id DESC);

-- 8) 重建 FTS：外部内容表不会跟随表名自动更新，必须显式重建并回填。
CREATE VIRTUAL TABLE captures_fts USING fts5(
    title,
    summary,
    note,
    body,
    content='captures',
    content_rowid='rowid'
);

INSERT INTO captures_fts(rowid, title, summary, note, body)
SELECT rowid, title, summary, note, body FROM captures;

CREATE TRIGGER captures_fts_insert AFTER INSERT ON captures BEGIN
    INSERT INTO captures_fts(rowid, title, summary, note, body)
    VALUES (new.rowid, new.title, new.summary, new.note, new.body);
END;

CREATE TRIGGER captures_fts_delete AFTER DELETE ON captures BEGIN
    INSERT INTO captures_fts(captures_fts, rowid, title, summary, note, body)
    VALUES ('delete', old.rowid, old.title, old.summary, old.note, old.body);
END;

CREATE TRIGGER captures_fts_update AFTER UPDATE OF title, summary, note, body ON captures BEGIN
    INSERT INTO captures_fts(captures_fts, rowid, title, summary, note, body)
    VALUES ('delete', old.rowid, old.title, old.summary, old.note, old.body);
    INSERT INTO captures_fts(rowid, title, summary, note, body)
    VALUES (new.rowid, new.title, new.summary, new.note, new.body);
END;
