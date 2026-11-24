CREATE TABLE items (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('note', 'link', 'github')),
    source TEXT NOT NULL,
    url TEXT,
    title TEXT,
    summary TEXT,
    note TEXT,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'processed', 'archived')),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX items_status_created_at ON items(status, created_at DESC);

CREATE TABLE tags (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    color TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE item_tags (
    item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (item_id, tag_id)
);

CREATE TABLE github_repos (
    id TEXT PRIMARY KEY NOT NULL,
    owner TEXT NOT NULL,
    name TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    last_synced_at TEXT,
    etag TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE(owner COLLATE NOCASE, name COLLATE NOCASE)
);

CREATE TABLE github_events (
    id TEXT PRIMARY KEY NOT NULL,
    repo_id TEXT NOT NULL REFERENCES github_repos(id) ON DELETE CASCADE,
    source_event_id TEXT NOT NULL,
    event_type TEXT NOT NULL CHECK (event_type IN ('release', 'issue', 'pull_request')),
    payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
    occurred_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE(repo_id, source_event_id)
);

CREATE INDEX github_events_repo_occurred_at ON github_events(repo_id, occurred_at DESC);

CREATE TABLE sync_runs (
    id TEXT PRIMARY KEY NOT NULL,
    source TEXT NOT NULL,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    status TEXT NOT NULL DEFAULT 'running'
        CHECK (status IN ('running', 'succeeded', 'failed')),
    error TEXT
);

CREATE INDEX sync_runs_source_started_at ON sync_runs(source, started_at DESC);

CREATE VIRTUAL TABLE items_fts USING fts5(
    title,
    summary,
    note,
    content='items',
    content_rowid='rowid'
);

CREATE TRIGGER items_fts_insert AFTER INSERT ON items BEGIN
    INSERT INTO items_fts(rowid, title, summary, note)
    VALUES (new.rowid, new.title, new.summary, new.note);
END;

CREATE TRIGGER items_fts_delete AFTER DELETE ON items BEGIN
    INSERT INTO items_fts(items_fts, rowid, title, summary, note)
    VALUES ('delete', old.rowid, old.title, old.summary, old.note);
END;

CREATE TRIGGER items_fts_update AFTER UPDATE OF title, summary, note ON items BEGIN
    INSERT INTO items_fts(items_fts, rowid, title, summary, note)
    VALUES ('delete', old.rowid, old.title, old.summary, old.note);
    INSERT INTO items_fts(rowid, title, summary, note)
    VALUES (new.rowid, new.title, new.summary, new.note);
END;
