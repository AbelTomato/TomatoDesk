-- 工作站形态：GitHub 内容脱离 items，按仓库组织。
-- 迁移前由 database.rs 自动备份数据库到 <数据目录>/backup/，迁移后需要重新同步一次。

-- 1) 个人条目可以挂到某个仓库下（仓库详情的「我的条目」）。
--    SQLite 只允许 ADD COLUMN 时带 NULL 默认值的外键列。
ALTER TABLE items ADD COLUMN repo_id TEXT REFERENCES github_repos(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS items_repo_updated_at_id
    ON items(repo_id, updated_at DESC, id DESC);

-- 2) 仓库元信息：来自 /repos/{owner}/{name} 与 README，custom_* 为用户自定义覆盖。
ALTER TABLE github_repos ADD COLUMN description TEXT;
ALTER TABLE github_repos ADD COLUMN homepage TEXT;
ALTER TABLE github_repos ADD COLUMN language TEXT;
ALTER TABLE github_repos ADD COLUMN stars INTEGER;
ALTER TABLE github_repos ADD COLUMN forks INTEGER;
ALTER TABLE github_repos ADD COLUMN open_issues INTEGER;
ALTER TABLE github_repos ADD COLUMN default_branch TEXT;
ALTER TABLE github_repos ADD COLUMN pushed_at TEXT;
ALTER TABLE github_repos ADD COLUMN is_archived INTEGER NOT NULL DEFAULT 0
    CHECK (is_archived IN (0, 1));
ALTER TABLE github_repos ADD COLUMN is_private INTEGER NOT NULL DEFAULT 0
    CHECK (is_private IN (0, 1));
-- README 只存原文，不渲染 HTML；summary 是抽取出来的纯文本摘要。
ALTER TABLE github_repos ADD COLUMN readme_text TEXT;
ALTER TABLE github_repos ADD COLUMN readme_summary TEXT;
ALTER TABLE github_repos ADD COLUMN readme_synced_at TEXT;
ALTER TABLE github_repos ADD COLUMN custom_description TEXT;
ALTER TABLE github_repos ADD COLUMN custom_note TEXT;
ALTER TABLE github_repos ADD COLUMN meta_synced_at TEXT;

-- 3) PR / Issue 统一为「线程」，按 (仓库, 类型, 编号) 去重。
CREATE TABLE github_threads (
    id TEXT PRIMARY KEY NOT NULL,
    repo_id TEXT NOT NULL REFERENCES github_repos(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('pull_request', 'issue')),
    number INTEGER NOT NULL,
    title TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('open', 'draft', 'merged', 'closed')),
    author TEXT,
    url TEXT NOT NULL,
    body TEXT,
    summary TEXT,
    -- 四条关系都相对同步时使用的 Token 所属用户，互不排斥。
    is_author INTEGER NOT NULL DEFAULT 0 CHECK (is_author IN (0, 1)),
    is_assigned INTEGER NOT NULL DEFAULT 0 CHECK (is_assigned IN (0, 1)),
    review_requested INTEGER NOT NULL DEFAULT 0 CHECK (review_requested IN (0, 1)),
    reviewed INTEGER NOT NULL DEFAULT 0 CHECK (reviewed IN (0, 1)),
    comment_count INTEGER NOT NULL DEFAULT 0,
    additions INTEGER,
    deletions INTEGER,
    changed_files INTEGER,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    closed_at TEXT,
    merged_at TEXT,
    synced_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE(repo_id, kind, number)
);

CREATE INDEX IF NOT EXISTS github_threads_repo_updated_at
    ON github_threads(repo_id, updated_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS github_threads_repo_kind_updated_at
    ON github_threads(repo_id, kind, updated_at DESC, id DESC);

-- 4) Release 没有编号，单独成表。
CREATE TABLE github_releases (
    id TEXT PRIMARY KEY NOT NULL,
    repo_id TEXT NOT NULL REFERENCES github_repos(id) ON DELETE CASCADE,
    tag_name TEXT NOT NULL,
    name TEXT,
    url TEXT NOT NULL,
    body TEXT,
    summary TEXT,
    is_draft INTEGER NOT NULL DEFAULT 0 CHECK (is_draft IN (0, 1)),
    is_prerelease INTEGER NOT NULL DEFAULT 0 CHECK (is_prerelease IN (0, 1)),
    published_at TEXT,
    created_at TEXT NOT NULL,
    synced_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE(repo_id, tag_name)
);

CREATE INDEX IF NOT EXISTS github_releases_repo_published_at
    ON github_releases(repo_id, published_at DESC, created_at DESC);

-- 5) 清理旧形态：GitHub 条目与事件载荷都搬进上面两张表，旧数据不可恢复。
--    items.kind 的 CHECK 仍保留 'github' 取值，作为已知技术债，避免重建整表。
DELETE FROM items WHERE kind = 'github';
DROP TABLE IF EXISTS github_events;
