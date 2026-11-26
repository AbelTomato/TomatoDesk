-- PR / Issue 的对话评论：按需拉取（打开详情时），本地缓存，离线可读。
-- PR 的对话评论与 Issue 共用 /repos/{owner}/{repo}/issues/{number}/comments；
-- 代码行级 review comments 不在本次范围内。

ALTER TABLE github_threads ADD COLUMN comments_synced_at TEXT;

CREATE TABLE github_thread_comments (
    id TEXT PRIMARY KEY NOT NULL,
    thread_id TEXT NOT NULL REFERENCES github_threads(id) ON DELETE CASCADE,
    github_id INTEGER NOT NULL,
    author TEXT,
    body TEXT NOT NULL,
    url TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    synced_at TEXT NOT NULL,
    UNIQUE(thread_id, github_id)
);

CREATE INDEX IF NOT EXISTS github_thread_comments_thread_created_at
    ON github_thread_comments(thread_id, created_at);
