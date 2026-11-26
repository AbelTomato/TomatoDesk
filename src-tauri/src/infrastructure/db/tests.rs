use super::Database;
use crate::domain::{
    CaptureKind, CapturePageQuery, CaptureSort, CaptureSummary, GithubRelease, GithubThread,
    GithubThreadComment, KeysetCursor, NewCapture, NewSyncRun, SyncRunStatus, ThreadKind,
    ThreadQuery, ThreadRelationship, ThreadState, ThreadSummary,
};
use uuid::Uuid;

fn capture(title: &str, body: &str) -> NewCapture {
    NewCapture {
        kind: CaptureKind::Note,
        source: "manual".into(),
        url: None,
        title: Some(title.into()),
        summary: None,
        note: None,
        body: Some(body.into()),
        repo_id: None,
    }
}

fn thread(repo_id: &str, number: i64, state: ThreadState) -> GithubThread {
    GithubThread {
        id: Uuid::new_v4().to_string(),
        repo_id: repo_id.into(),
        kind: ThreadKind::PullRequest,
        number,
        title: format!("PR #{number}"),
        state,
        author: Some("someone".into()),
        url: format!("https://github.com/owner/project/pull/{number}"),
        body: Some("正文".into()),
        summary: Some("正文".into()),
        is_author: false,
        is_assigned: false,
        review_requested: true,
        reviewed: false,
        comment_count: 2,
        additions: Some(10),
        deletions: Some(2),
        changed_files: Some(3),
        created_at: "2026-11-20T00:00:00Z".into(),
        updated_at: "2026-11-25T00:00:00Z".into(),
        closed_at: None,
        merged_at: None,
        synced_at: "2026-11-25T00:00:00Z".into(),
    }
}

fn release(repo_id: &str) -> GithubRelease {
    GithubRelease {
        id: Uuid::new_v4().to_string(),
        repo_id: repo_id.into(),
        tag_name: "v0.3.0".into(),
        name: Some("v0.3.0".into()),
        url: "https://github.com/owner/project/releases/tag/v0.3.0".into(),
        body: Some("发布说明".into()),
        summary: Some("发布说明".into()),
        is_draft: false,
        is_prerelease: false,
        published_at: Some("2026-11-24T00:00:00Z".into()),
        created_at: "2026-11-24T00:00:00Z".into(),
        synced_at: "2026-11-25T00:00:00Z".into(),
    }
}

#[test]
fn migrations_are_idempotent_and_captures_support_crud() {
    let db = Database::in_memory().unwrap();
    db.migrate().unwrap();
    let migration_count: i64 = db
        .with_connection(|connection| {
            Ok(
                connection.query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                    row.get(0)
                })?,
            )
        })
        .unwrap();
    assert_eq!(migration_count, 5);
    let captures = db.captures();
    let created = captures
        .create(capture("First title", "first body"))
        .unwrap();
    assert_eq!(captures.get(&created.id).unwrap(), Some(created.clone()));
    assert_eq!(captures.list().unwrap(), vec![created.clone()]);

    let mut changed = created.clone();
    changed.title = Some("Updated title".into());
    captures.update(&changed).unwrap();
    let updated = captures.get(&created.id).unwrap().unwrap();
    assert_eq!(updated.title.as_deref(), Some("Updated title"));
    assert_eq!(updated.archived_at, None);

    assert!(captures.delete(&created.id).unwrap());
    assert_eq!(captures.get(&created.id).unwrap(), None);
    assert!(!captures.delete(&created.id).unwrap());
}

#[test]
fn database_creates_parent_directories_and_persists_captures() {
    let temp_dir = std::env::temp_dir().join(format!("tomatodesk-db-{}", Uuid::new_v4()));
    let database_path = temp_dir.join("nested/tomatodesk.sqlite3");
    let db = Database::open(&database_path).unwrap();
    assert_eq!(db.path(), Some(database_path.as_path()));
    let created = db
        .captures()
        .create(capture("Persistent", "survives reopen"))
        .unwrap();
    drop(db);

    let reopened = Database::open(&database_path).unwrap();
    assert_eq!(reopened.captures().get(&created.id).unwrap(), Some(created));
    assert_eq!(
        reopened.migrated_to(),
        None,
        "a second open has nothing to migrate"
    );
    drop(reopened);
    std::fs::remove_dir_all(temp_dir).unwrap();
}

#[test]
fn fts_tracks_capture_insert_update_and_delete() {
    let db = Database::in_memory().unwrap();
    let captures = db.captures();
    let created = captures
        .create(capture("Rust handbook", "keep this body searchable"))
        .unwrap();
    assert_eq!(captures.search("handbook").unwrap().len(), 1);
    assert_eq!(captures.search("searchable").unwrap().len(), 1);

    let mut changed = created.clone();
    changed.title = Some("SQLite guide".into());
    changed.body = Some("updated searchable content".into());
    captures.update(&changed).unwrap();
    assert!(captures.search("handbook").unwrap().is_empty());
    assert_eq!(captures.search("SQLite").unwrap()[0].id, created.id);

    captures.delete(&created.id).unwrap();
    assert!(captures.search("SQLite").unwrap().is_empty());
}

#[test]
fn tags_and_capture_tag_links_obey_foreign_keys() {
    let db = Database::in_memory().unwrap();
    let captures = db.captures();
    let tags = db.tags();
    let created = captures.create(capture("Tagged", "body")).unwrap();
    let tag = tags.create("Reading", Some("#3b82f6")).unwrap();

    tags.set_capture_tags(&created.id, std::slice::from_ref(&tag.id))
        .unwrap();
    assert_eq!(tags.for_capture(&created.id).unwrap(), vec![tag.clone()]);
    assert_eq!(tags.get(&tag.id).unwrap(), Some(tag.clone()));
    assert_eq!(tags.list().unwrap(), vec![tag.clone()]);
    assert!(
        tags.set_capture_tags("missing-capture", std::slice::from_ref(&tag.id))
            .is_err()
    );

    captures.delete(&created.id).unwrap();
    assert!(tags.for_capture(&created.id).unwrap().is_empty());
    assert!(tags.delete(&tag.id).unwrap());
}

#[test]
fn repo_workspace_upserts_threads_releases_and_counts_by_relation() {
    let db = Database::in_memory().unwrap();
    let repos = db.github_repos();
    let repo = repos.create("owner", "project").unwrap();

    let mut updated = repo.clone();
    updated.description = Some("本地优先的信息收件箱".into());
    updated.language = Some("Rust".into());
    updated.readme_summary = Some("README 摘要".into());
    updated.last_synced_at = Some("2026-11-25T00:00:00Z".into());
    updated.meta_synced_at = Some("2026-11-25T00:00:00Z".into());

    let first = thread(&repo.id, 1, ThreadState::Open);
    let merged = GithubThread {
        state: ThreadState::Merged,
        review_requested: false,
        reviewed: true,
        ..thread(&repo.id, 2, ThreadState::Merged)
    };
    let release = release(&repo.id);

    let writes = repos
        .apply_sync(
            &updated,
            &[first.clone(), merged.clone()],
            std::slice::from_ref(&release),
        )
        .unwrap();
    assert_eq!(writes.threads_upserted, 2);
    assert_eq!(writes.releases_upserted, 1);

    // 同一批内容再写一次，没有任何行变化。
    let writes = repos
        .apply_sync(
            &updated,
            &[first.clone(), merged.clone()],
            std::slice::from_ref(&release),
        )
        .unwrap();
    assert_eq!(
        writes.threads_upserted, 0,
        "unchanged threads must not write"
    );
    assert_eq!(writes.releases_upserted, 0);

    // 状态变化才算变化，并且 (repo, kind, number) 不会产生重复行。
    let closed = GithubThread {
        state: ThreadState::Closed,
        ..first.clone()
    };
    let writes = repos.apply_sync(&updated, &[closed], &[]).unwrap();
    assert_eq!(writes.threads_upserted, 1);

    let page = db
        .github_threads()
        .page(&ThreadQuery {
            repo_id: repo.id.clone(),
            kind: None,
            state: None,
            relationship: None,
            cursor: None,
            limit: 10,
        })
        .unwrap();
    assert_eq!(page.threads.len(), 2);

    let reviews = db
        .github_threads()
        .page(&ThreadQuery {
            repo_id: repo.id.clone(),
            kind: Some(ThreadKind::PullRequest),
            state: None,
            relationship: Some(ThreadRelationship::Reviewed),
            cursor: None,
            limit: 10,
        })
        .unwrap();
    assert_eq!(reviews.threads.len(), 1);
    assert_eq!(reviews.threads[0].number, 2);

    let detail = repos.detail(&repo.id).unwrap().unwrap();
    assert_eq!(detail.repo.readme_summary.as_deref(), Some("README 摘要"));
    assert_eq!(
        detail.counts.open_pull_requests, 0,
        "both threads are closed"
    );
    assert_eq!(detail.counts.releases, 1);

    // 随手记可以挂到仓库下，并在详情里单独计数。
    let attached = db
        .captures()
        .create(NewCapture {
            repo_id: Some(repo.id.clone()),
            ..capture("仓库笔记", "记一条")
        })
        .unwrap();
    assert_eq!(attached.repo_id.as_deref(), Some(repo.id.as_str()));
    let captures = db
        .captures()
        .page(&CapturePageQuery {
            archived: None,
            kind: None,
            repo_id: Some(repo.id.clone()),
            sort: CaptureSort::Updated,
            cursor: None,
            limit: 10,
        })
        .unwrap();
    assert_eq!(captures.captures, vec![CaptureSummary::from(&attached)]);
    assert_eq!(repos.counts(&repo.id).unwrap().captures, 1);

    assert_eq!(
        db.github_releases().list_for_repo(&repo.id).unwrap().len(),
        1
    );

    // 删除仓库会连带清掉线程与 Release，随手记保留但解除关联。
    assert!(repos.delete(&repo.id).unwrap());
    assert!(db.github_threads().get(&first.id).unwrap().is_none());
    assert!(
        db.github_releases()
            .list_for_repo(&repo.id)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        db.captures().get(&attached.id).unwrap().unwrap().repo_id,
        None
    );
}

#[test]
fn thread_comments_are_replaced_in_order_and_cascade_with_the_thread() {
    let db = Database::in_memory().unwrap();
    let repos = db.github_repos();
    let repo = repos.create("owner", "project").unwrap();
    let pr = thread(&repo.id, 7, ThreadState::Open);
    repos
        .apply_sync(&repo, std::slice::from_ref(&pr), &[])
        .unwrap();

    let comment = |github_id: i64, created_at: &str, body: &str| GithubThreadComment {
        id: Uuid::new_v4().to_string(),
        thread_id: pr.id.clone(),
        github_id,
        author: Some("someone".into()),
        body: body.into(),
        url: format!("https://github.com/owner/project/pull/7#issuecomment-{github_id}"),
        created_at: created_at.into(),
        updated_at: created_at.into(),
        synced_at: "2026-11-25T01:00:00Z".into(),
    };

    // 传入顺序故意打乱，读出来必须按创建时间升序。
    let comments = vec![
        comment(2, "2026-11-25T00:02:00Z", "第二条"),
        comment(1, "2026-11-25T00:01:00Z", "第一条"),
    ];
    db.github_threads()
        .replace_comments(&pr.id, &comments, "2026-11-25T01:00:00Z")
        .unwrap();

    let detail = db.github_threads().get(&pr.id).unwrap().unwrap();
    assert_eq!(
        detail.comments_synced_at.as_deref(),
        Some("2026-11-25T01:00:00Z")
    );
    assert_eq!(
        detail
            .comments
            .iter()
            .map(|comment| comment.github_id)
            .collect::<Vec<_>>(),
        vec![1, 2],
        "comments are ordered by created_at"
    );

    // 再次替换：GitHub 侧删除的评论不能残留。
    db.github_threads()
        .replace_comments(
            &pr.id,
            &[comment(1, "2026-11-25T00:01:00Z", "改过的正文")],
            "2026-11-25T02:00:00Z",
        )
        .unwrap();
    let detail = db.github_threads().get(&pr.id).unwrap().unwrap();
    assert_eq!(detail.comments.len(), 1);
    assert_eq!(detail.comments[0].body, "改过的正文");

    // 删除仓库连带删除线程与评论。
    repos.delete(&repo.id).unwrap();
    let remaining: i64 = db
        .with_connection(|connection| {
            Ok(
                connection.query_row("SELECT COUNT(*) FROM github_thread_comments", [], |row| {
                    row.get(0)
                })?,
            )
        })
        .unwrap();
    assert_eq!(remaining, 0);
}

#[test]
fn thread_pages_use_keyset_cursors_without_duplicates() {
    let db = Database::in_memory().unwrap();
    let repos = db.github_repos();
    let repo = repos.create("owner", "project").unwrap();
    let mut updated = repo.clone();
    updated.last_synced_at = Some("2026-11-25T00:00:00Z".into());

    let threads = (1..=5)
        .map(|number| GithubThread {
            updated_at: format!("2026-11-2{number}T00:00:00Z"),
            ..thread(&repo.id, number, ThreadState::Open)
        })
        .collect::<Vec<_>>();
    repos.apply_sync(&updated, &threads, &[]).unwrap();

    let mut collected: Vec<ThreadSummary> = Vec::new();
    let mut cursor: Option<KeysetCursor> = None;
    loop {
        let page = db
            .github_threads()
            .page(&ThreadQuery {
                repo_id: repo.id.clone(),
                kind: None,
                state: None,
                relationship: None,
                cursor: cursor.clone(),
                limit: 2,
            })
            .unwrap();
        collected.extend(page.threads);
        match page.next_cursor {
            Some(next) if page.has_more => cursor = KeysetCursor::parse(&next),
            _ => break,
        }
    }
    assert_eq!(collected.len(), 5);
    let numbers = collected
        .iter()
        .map(|entry| entry.number)
        .collect::<Vec<_>>();
    assert_eq!(numbers, vec![5, 4, 3, 2, 1], "newest first, no duplicates");
}

#[test]
fn sync_runs_can_finish_and_list_latest() {
    let db = Database::in_memory().unwrap();
    let runs = db.sync_runs();
    let run = runs
        .create(NewSyncRun {
            source: "github".into(),
            started_at: "2026-11-24T00:00:00Z".into(),
        })
        .unwrap();
    assert_eq!(run.status, SyncRunStatus::Running);
    let mut finished = run.clone();
    finished.status = SyncRunStatus::Succeeded;
    finished.finished_at = Some("2026-11-24T00:00:03Z".into());
    runs.update(&finished).unwrap();
    assert_eq!(
        runs.get(&run.id).unwrap().unwrap().status,
        SyncRunStatus::Succeeded
    );
    assert_eq!(runs.list_latest("github", 10).unwrap().len(), 1);
}

#[test]
fn body_migration_splits_long_summaries_and_indexes_body() {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    connection
        .execute_batch(include_str!("../../../migrations/0001_initial.sql"))
        .unwrap();
    let long_summary = "alpha ".repeat(100);
    connection
        .execute(
            "INSERT INTO items(id, kind, source, summary) VALUES ('a', 'note', 'manual', ?1)",
            [&long_summary],
        )
        .unwrap();

    connection
        .execute_batch(include_str!("../../../migrations/0002_item_body.sql"))
        .unwrap();

    let (body, summary): (Option<String>, Option<String>) = connection
        .query_row(
            "SELECT body, summary FROM items WHERE id = 'a'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(body.as_deref(), Some(long_summary.as_str()));

    let summary = summary.unwrap();
    assert_eq!(
        summary.chars().count(),
        201,
        "200 characters plus an ellipsis"
    );
    assert!(summary.ends_with('…'));

    // 重建后的 FTS 覆盖正文，正文里的词也能命中。
    let hits: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM items_fts WHERE items_fts MATCH ?1",
            ["\"alpha\""],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(hits, 1);
}

#[test]
fn workspace_migration_moves_github_items_out_of_the_inbox() {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    connection
        .execute_batch(include_str!("../../../migrations/0001_initial.sql"))
        .unwrap();
    connection
        .execute_batch(include_str!("../../../migrations/0002_item_body.sql"))
        .unwrap();
    connection
        .execute_batch(
            "INSERT INTO github_repos(id, owner, name) VALUES ('repo', 'owner', 'project');
             INSERT INTO github_events(id, repo_id, source_event_id, event_type, payload_json, occurred_at)
                 VALUES ('event', 'repo', 'issue:1', 'issue', '{\"number\":1}', '2026-11-24T00:00:00Z');
             INSERT INTO items(id, kind, source, title) VALUES ('gh', 'github', 'github:owner/project', 'PR #1');
             INSERT INTO items(id, kind, source, note) VALUES ('note', 'note', 'manual', '保留');",
        )
        .unwrap();

    connection
        .execute_batch(include_str!("../../../migrations/0003_repo_workspace.sql"))
        .unwrap();

    let github_items: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM items WHERE kind = 'github'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(github_items, 0, "legacy GitHub items are removed");
    let remaining: i64 = connection
        .query_row("SELECT COUNT(*) FROM items WHERE id = 'note'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(remaining, 1, "personal items survive");

    let legacy_events: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'github_events'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(legacy_events, 0, "github_events is dropped");

    let threads: i64 = connection
        .query_row("SELECT COUNT(*) FROM github_threads", [], |row| row.get(0))
        .unwrap();
    assert_eq!(threads, 0);
    let repo_columns: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('github_repos')
             WHERE name = 'custom_description'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(repo_columns, 1);
    let item_columns: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('items') WHERE name = 'repo_id'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(item_columns, 1);
}

#[test]
fn capture_migration_maps_status_note_body_and_tags() {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .unwrap();
    for migration in [
        include_str!("../../../migrations/0001_initial.sql"),
        include_str!("../../../migrations/0002_item_body.sql"),
        include_str!("../../../migrations/0003_repo_workspace.sql"),
    ] {
        connection.execute_batch(migration).unwrap();
    }
    connection
        .execute_batch(
            "INSERT INTO tags(id, name) VALUES ('tag', 'Reading');
             INSERT INTO items(id, kind, source, title, note, status)
                 VALUES ('note', 'note', 'manual', 'Note', 'searchable body', 'pending');
             INSERT INTO items(id, kind, source, url, title, summary, note, status)
                 VALUES ('link', 'link', 'clipboard', 'https://example.com', 'Link', '摘要', '备注', 'archived');
             INSERT INTO items(id, kind, source, note, status)
                 VALUES ('processed', 'note', 'manual', 'processed body', 'processed');
             INSERT INTO item_tags(item_id, tag_id) VALUES ('note', 'tag');",
        )
        .unwrap();

    connection
        .execute_batch(include_str!("../../../migrations/0005_capture_module.sql"))
        .unwrap();

    let items_table: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'items'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(items_table, 0, "items is renamed to captures");
    let status_column: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('captures') WHERE name = 'status'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(status_column, 0, "status is removed");

    let (body, note, summary, archived_at): (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = connection
        .query_row(
            "SELECT body, note, summary, archived_at FROM captures WHERE id = 'note'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(body.as_deref(), Some("searchable body"));
    assert_eq!(note, None);
    assert_eq!(summary.as_deref(), Some("searchable body"));
    assert_eq!(archived_at, None, "pending becomes active");

    let (body, note, source, archived_at): (
        Option<String>,
        Option<String>,
        String,
        Option<String>,
    ) = connection
        .query_row(
            "SELECT body, note, source, archived_at FROM captures WHERE id = 'link'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(body, None);
    assert_eq!(note.as_deref(), Some("备注"));
    assert_eq!(source, "quick_capture");
    assert!(archived_at.is_some(), "archived keeps its timestamp");

    let archived_at: Option<String> = connection
        .query_row(
            "SELECT archived_at FROM captures WHERE id = 'processed'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(archived_at, None, "processed becomes active");

    let fts_hits: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM captures_fts WHERE captures_fts MATCH ?1",
            ["\"searchable\""],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(fts_hits, 1, "FTS is rebuilt against captures");

    let tag_links: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM capture_tags WHERE capture_id = 'note' AND tag_id = 'tag'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(tag_links, 1, "tag links survive the rename");
}

