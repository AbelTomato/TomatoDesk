-- 正文与摘要分离：
-- body 保存完整正文，只在详情面板按需读取；
-- summary 退化为列表用的短摘要，最多 200 字。
ALTER TABLE items ADD COLUMN body TEXT;

UPDATE items
SET body = summary,
    summary = CASE
        WHEN length(summary) > 200 THEN substr(summary, 1, 200) || '…'
        ELSE summary
    END
WHERE summary IS NOT NULL;

-- 旧 FTS 索引只覆盖 title/summary/note，重建为包含 body。
DROP TRIGGER IF EXISTS items_fts_insert;
DROP TRIGGER IF EXISTS items_fts_delete;
DROP TRIGGER IF EXISTS items_fts_update;
DROP TABLE IF EXISTS items_fts;

CREATE VIRTUAL TABLE items_fts USING fts5(
    title,
    summary,
    note,
    body,
    content='items',
    content_rowid='rowid'
);

INSERT INTO items_fts(rowid, title, summary, note, body)
SELECT rowid, title, summary, note, body FROM items;

CREATE TRIGGER items_fts_insert AFTER INSERT ON items BEGIN
    INSERT INTO items_fts(rowid, title, summary, note, body)
    VALUES (new.rowid, new.title, new.summary, new.note, new.body);
END;

CREATE TRIGGER items_fts_delete AFTER DELETE ON items BEGIN
    INSERT INTO items_fts(items_fts, rowid, title, summary, note, body)
    VALUES ('delete', old.rowid, old.title, old.summary, old.note, old.body);
END;

CREATE TRIGGER items_fts_update AFTER UPDATE OF title, summary, note, body ON items BEGIN
    INSERT INTO items_fts(items_fts, rowid, title, summary, note, body)
    VALUES ('delete', old.rowid, old.title, old.summary, old.note, old.body);
    INSERT INTO items_fts(rowid, title, summary, note, body)
    VALUES (new.rowid, new.title, new.summary, new.note, new.body);
END;

-- 键集分页：(updated_at, id) 与 (created_at, id) 两个排序方向都要能用上索引。
CREATE INDEX IF NOT EXISTS items_updated_at_id ON items(updated_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS items_status_updated_at_id ON items(status, updated_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS items_kind_updated_at_id ON items(kind, updated_at DESC, id DESC);
