use super::{Database, MANUAL_SOURCE_ID};
use crate::domain::{
    CaptureKind, CapturePageQuery, CaptureSort, CaptureSummary, EmailTls, GithubRelease,
    GithubThread, GithubThreadComment, InboxAction, InboxPageQuery, InboxScope, InboxSourceKind,
    InboxTab, KeysetCursor, NewCapture, NewInboxMessage, NewSyncRun, NewTask, ReadBackErrorKind,
    SyncRunStatus, TaskPageQuery, TaskStatus, ThreadKind, ThreadQuery, ThreadRelationship,
    ThreadState, ThreadSummary, TriageOutcome,
};
use crate::infrastructure::email::{EmailConfig, account_id};
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
    assert_eq!(migration_count, 9);
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

fn new_task(title: &str, origin_kind: Option<&str>, origin_id: Option<&str>) -> NewTask {
    NewTask {
        title: title.into(),
        note: None,
        origin_kind: origin_kind.map(Into::into),
        origin_id: origin_id.map(Into::into),
    }
}

#[test]
fn task_repository_tracks_status_counts_and_completion_time() {
    let db = Database::in_memory().unwrap();
    let tasks = db.tasks();

    let first = tasks
        .create(new_task("写阶段二", Some("manual"), None))
        .unwrap();
    let second = tasks
        .create(new_task("整理文档", Some("manual"), None))
        .unwrap();
    assert_eq!(first.status, TaskStatus::Open);
    assert_eq!(first.completed_at, None);

    let page = tasks
        .page(&TaskPageQuery {
            status: None,
            cursor: None,
            limit: 50,
        })
        .unwrap();
    assert_eq!(page.tasks.len(), 2);
    assert!(!page.has_more);

    let done = tasks
        .set_status(&first.id, TaskStatus::Done)
        .unwrap()
        .unwrap();
    let completed_at = done.completed_at.clone().expect("completing stamps a time");
    let done_again = tasks
        .set_status(&first.id, TaskStatus::Done)
        .unwrap()
        .unwrap();
    assert_eq!(
        done_again.completed_at,
        Some(completed_at),
        "completing twice keeps the first completion time"
    );

    let cancelled = tasks
        .set_status(&first.id, TaskStatus::Cancelled)
        .unwrap()
        .unwrap();
    assert_eq!(cancelled.status, TaskStatus::Cancelled);
    assert_eq!(cancelled.completed_at, None, "cancelling clears completion");

    let reopened = tasks
        .set_status(&first.id, TaskStatus::Open)
        .unwrap()
        .unwrap();
    assert_eq!(reopened.status, TaskStatus::Open);
    assert_eq!(reopened.completed_at, None);

    let updated = tasks
        .update(&second.id, "整理发布说明", Some("改标题"))
        .unwrap()
        .unwrap();
    assert_eq!(updated.title, "整理发布说明");
    assert_eq!(updated.note.as_deref(), Some("改标题"));

    let counts = tasks.counts().unwrap();
    assert_eq!(
        (counts.all, counts.open, counts.done, counts.cancelled),
        (2, 2, 0, 0)
    );

    let done_page = tasks
        .page(&TaskPageQuery {
            status: Some(TaskStatus::Done),
            cursor: None,
            limit: 50,
        })
        .unwrap();
    assert!(done_page.tasks.is_empty(), "status filter runs in SQL");

    assert!(
        tasks
            .set_status("missing", TaskStatus::Done)
            .unwrap()
            .is_none()
    );
    assert!(
        tasks.create(new_task("   ", Some("manual"), None)).is_err(),
        "blank titles are rejected"
    );
}

#[test]
fn task_origin_index_allows_only_one_task_per_inbox_message() {
    let db = Database::in_memory().unwrap();
    let tasks = db.tasks();
    let origin = new_task("来自收件箱", Some("inbox"), Some("message-1"));

    let created = tasks.create(origin.clone()).unwrap();
    assert_eq!(created.origin_kind.as_deref(), Some("inbox"));
    assert!(
        tasks.create(origin).is_err(),
        "the unique index blocks a second task for the same message"
    );

    let manual = new_task("手动待办", Some("manual"), Some("message-1"));
    assert!(
        tasks.create(manual).is_ok(),
        "the unique index only covers inbox origins"
    );
}

/// 单条动作的入参：写成 `&[id.clone()]` 会被 clippy 判成多余的克隆。
fn only(id: &str) -> Vec<String> {
    vec![id.to_owned()]
}

fn inbox_message(external_id: &str, title: &str, updated_at: &str) -> NewInboxMessage {
    NewInboxMessage {
        source_id: MANUAL_SOURCE_ID.into(),
        external_id: external_id.into(),
        external_updated_at: Some(updated_at.into()),
        category: "mention".into(),
        title: title.into(),
        summary: Some("摘要".into()),
        body: None,
        url: Some(format!("https://example.com/{external_id}")),
        actor: Some("reviewer".into()),
        occurred_at: Some(updated_at.into()),
        related_kind: None,
        related_id: None,
        payload_json: None,
    }
}

#[test]
fn inbox_repository_dedupes_messages_and_converts_once() {
    let db = Database::in_memory().unwrap();
    let sources = db.inbox_sources();
    let source = sources.ensure_manual().unwrap();
    assert_eq!(source.id, MANUAL_SOURCE_ID);
    assert_eq!(sources.ensure_manual().unwrap().id, source.id);
    assert_eq!(sources.list().unwrap().len(), 1, "manual source is reused");

    let messages = db.inbox_messages();
    let created = messages
        .ingest(inbox_message(
            "notice-1",
            "被 @ 了",
            "2026-11-26T01:00:00.000Z",
        ))
        .unwrap();
    assert_eq!(created.triaged_at, None);

    let updated = messages
        .ingest(inbox_message(
            "notice-1",
            "被 @ 了（更新）",
            "2026-11-26T02:00:00.000Z",
        ))
        .unwrap();
    assert_eq!(updated.id, created.id, "the same key updates in place");
    assert_eq!(updated.title, "被 @ 了（更新）");
    assert_eq!(messages.counts(None, None).unwrap().all, 1);

    messages.set_read(&updated.id, true).unwrap();
    let unchanged = messages
        .ingest(inbox_message(
            "notice-1",
            "被 @ 了（更新）",
            "2026-11-26T02:00:00.000Z",
        ))
        .unwrap();
    assert!(
        unchanged.read_at.is_some(),
        "a sync without new progress keeps the message read"
    );

    let task = messages
        .apply_action(InboxAction::Task, &only(&updated.id))
        .unwrap()
        .task_ids
        .pop()
        .expect("conversion returns the created task id");
    let task = db.tasks().get(&task).unwrap().unwrap();
    assert_eq!(task.origin_id.as_deref(), Some(updated.id.as_str()));

    let triaged = messages.get(&updated.id).unwrap().unwrap();
    assert_eq!(triaged.triage_outcome, Some(TriageOutcome::Task));
    assert_eq!(triaged.triage_ref_id.as_deref(), Some(task.id.as_str()));
    assert_eq!(
        triaged.read_at, unchanged.read_at,
        "triage keeps read state"
    );

    let repeat = messages
        .apply_action(InboxAction::Task, &only(&updated.id))
        .unwrap();
    assert!(
        repeat.applied_ids.is_empty() && repeat.task_ids.is_empty() && repeat.skipped == 1,
        "a second conversion is skipped instead of creating another task"
    );
    assert_eq!(
        db.tasks().counts().unwrap().all,
        1,
        "only one task exists after a repeated conversion"
    );

    let dismissed = messages
        .ingest(inbox_message(
            "notice-2",
            "可以忽略",
            "2026-11-26T03:00:00.000Z",
        ))
        .unwrap();
    let dismissed = messages.dismiss(&dismissed.id).unwrap().unwrap();
    assert_eq!(dismissed.triage_outcome, Some(TriageOutcome::Dismissed));

    let counts = messages.counts(None, None).unwrap();
    assert_eq!(
        (counts.all, counts.pending),
        (2, 0),
        "both messages are triaged, so nothing is pending"
    );

    let refreshed = messages
        .ingest(inbox_message(
            "notice-2",
            "可以忽略（更新）",
            "2026-11-26T04:00:00.000Z",
        ))
        .unwrap();
    assert_eq!(
        refreshed.triage_outcome,
        Some(TriageOutcome::Dismissed),
        "re-ingest does not revive a dismissed message"
    );
}

/// 状态轴：四个 Tab 各自对应一段 SQL，计数与 Tab 无关、只随来源与分类变化。
#[test]
fn inbox_scope_splits_the_tabs_and_counts_add_up() {
    let db = Database::in_memory().unwrap();
    let manual = db.inbox_sources().ensure_manual().unwrap();
    let github = db.inbox_sources().ensure_github().unwrap();
    let messages = db.inbox_messages();

    let mention = messages
        .ingest(inbox_message("axis-1", "提及", "2026-11-26T01:00:00.000Z"))
        .unwrap();
    let dismissed = messages
        .ingest(inbox_message("axis-2", "忽略", "2026-11-26T02:00:00.000Z"))
        .unwrap();
    let converted = messages
        .ingest(inbox_message(
            "axis-3",
            "转待办",
            "2026-11-26T03:00:00.000Z",
        ))
        .unwrap();
    let release = messages
        .ingest(NewInboxMessage {
            source_id: github.id.clone(),
            category: "release".into(),
            ..inbox_message("axis-4", "发布", "2026-11-26T04:00:00.000Z")
        })
        .unwrap();

    messages.dismiss(&dismissed.id).unwrap();
    messages
        .apply_action(InboxAction::Task, &only(&converted.id))
        .unwrap();
    messages.set_read(&mention.id, true).unwrap();

    let all = messages.counts(None, None).unwrap();
    assert_eq!(
        (all.all, all.pending, all.task, all.dismissed, all.unread),
        (4, 2, 1, 1, 3),
        "the three outcome buckets add up to all; unread covers processed rows"
    );

    let manual_only = messages.counts(Some(&manual.id), None).unwrap();
    assert_eq!(
        (
            manual_only.all,
            manual_only.pending,
            manual_only.task,
            manual_only.dismissed,
            manual_only.unread
        ),
        (3, 1, 1, 1, 2),
        "counts follow the source filter"
    );

    let mentions = messages.counts(None, Some("mention")).unwrap();
    assert_eq!(
        (
            mentions.all,
            mentions.pending,
            mentions.task,
            mentions.dismissed
        ),
        (3, 1, 1, 1),
        "counts follow the category filter"
    );

    let page = |scope: InboxScope| {
        messages
            .page(&InboxPageQuery {
                scope,
                cursor: None,
                limit: 50,
            })
            .unwrap()
    };
    let ids = |query: &crate::domain::InboxPage| {
        query
            .messages
            .iter()
            .map(|message| message.id.clone())
            .collect::<Vec<_>>()
    };

    let pending = page(InboxScope::default());
    assert_eq!(ids(&pending), vec![release.id.clone(), mention.id.clone()]);

    let task = page(InboxScope {
        tab: InboxTab::Task,
        ..InboxScope::default()
    });
    assert_eq!(ids(&task), vec![converted.id.clone()]);

    let only_dismissed = page(InboxScope {
        tab: InboxTab::Dismissed,
        ..InboxScope::default()
    });
    assert_eq!(ids(&only_dismissed), vec![dismissed.id.clone()]);

    let unread = page(InboxScope {
        tab: InboxTab::All,
        unread_only: true,
        source_id: Some(manual.id.clone()),
        ..InboxScope::default()
    });
    assert_eq!(
        ids(&unread),
        vec![converted.id.clone(), dismissed.id.clone()],
        "unread is orthogonal to the status axis"
    );

    let everything = page(InboxScope {
        tab: InboxTab::All,
        ..InboxScope::default()
    });
    assert_eq!(
        everything.messages.len(),
        4,
        "the all tab applies no status condition"
    );
}

/// 批量动作：只统计真正改变的行，撤销回到原状态，解绑后能再次转换。
#[test]
fn inbox_actions_only_count_the_rows_they_change_and_can_be_undone() {
    let db = Database::in_memory().unwrap();
    db.inbox_sources().ensure_manual().unwrap();
    let messages = db.inbox_messages();
    let first = messages
        .ingest(inbox_message("act-1", "第一条", "2026-11-26T01:00:00.000Z"))
        .unwrap();
    let second = messages
        .ingest(inbox_message("act-2", "第二条", "2026-11-26T02:00:00.000Z"))
        .unwrap();

    let read = messages
        .apply_action(InboxAction::Read, &[first.id.clone(), second.id.clone()])
        .unwrap();
    assert_eq!(read.applied_ids, vec![first.id.clone(), second.id.clone()]);
    assert_eq!(read.skipped, 0);

    // 第二次标已读不改变任何行，所以不进 `applied_ids`：撤销才不会把它翻回未读。
    let again = messages
        .apply_action(InboxAction::Read, &[first.id.clone(), "missing".into()])
        .unwrap();
    assert!(again.applied_ids.is_empty());
    assert_eq!(again.skipped, 2);

    let undone = messages
        .undo_action(InboxAction::Read, &only(&first.id), &[])
        .unwrap();
    assert_eq!(undone.applied_ids, vec![first.id.clone()]);
    assert_eq!(messages.get(&first.id).unwrap().unwrap().read_at, None);

    let dismissed = messages
        .apply_action(InboxAction::Dismiss, &[first.id.clone(), second.id.clone()])
        .unwrap();
    assert_eq!(dismissed.applied_ids.len(), 2);
    assert_eq!(messages.counts(None, None).unwrap().pending, 0);

    let restored = messages
        .undo_action(
            InboxAction::Dismiss,
            &[first.id.clone(), second.id.clone()],
            &[],
        )
        .unwrap();
    assert_eq!(restored.applied_ids.len(), 2);
    assert_eq!(
        messages.counts(None, None).unwrap().pending,
        2,
        "undoing a dismissal puts both messages back to pending"
    );

    let converted = messages
        .apply_action(InboxAction::Task, &only(&first.id))
        .unwrap();
    let task_id = converted.task_ids.first().cloned().unwrap();
    assert_eq!(
        db.tasks()
            .get(&task_id)
            .unwrap()
            .unwrap()
            .origin_id
            .as_deref(),
        Some(first.id.as_str())
    );

    // 撤销「转待办」是解绑保留：待办还在，origin_id 置空，唯一索引不再拦第二次转换。
    let unbound = messages
        .undo_action(InboxAction::Task, &only(&first.id), &only(&task_id))
        .unwrap();
    assert_eq!(unbound.applied_ids, vec![first.id.clone()]);
    let task = db.tasks().get(&task_id).unwrap().unwrap();
    assert_eq!(task.origin_id, None, "the task is kept without a binding");
    assert_eq!(task.origin_kind.as_deref(), Some("inbox"));
    assert_eq!(messages.get(&first.id).unwrap().unwrap().triaged_at, None);

    let reconverted = messages
        .apply_action(InboxAction::Task, &only(&first.id))
        .unwrap();
    assert_eq!(reconverted.applied_ids, vec![first.id.clone()]);
    assert_eq!(db.tasks().counts().unwrap().all, 2);
}

/// 「选择全部 N 条」：条数对不上就一条都不改，对得上才按筛选解析 id。
#[test]
fn inbox_scope_actions_guard_on_the_expected_count() {
    let db = Database::in_memory().unwrap();
    db.inbox_sources().ensure_manual().unwrap();
    let messages = db.inbox_messages();
    for (index, category) in ["mention", "mention", "release"].iter().enumerate() {
        messages
            .ingest(NewInboxMessage {
                category: (*category).into(),
                ..inbox_message(
                    &format!("scope-{index}"),
                    &format!("消息 {index}"),
                    &format!("2026-11-26T0{index}:00:00.000Z"),
                )
            })
            .unwrap();
    }
    let scope = InboxScope::default();
    assert_eq!(messages.count_scope(&scope).unwrap(), 3);

    let stale = messages
        .apply_action_scope(InboxAction::Dismiss, &scope, 2)
        .unwrap();
    assert_eq!(stale.stale_count, Some(3));
    assert!(stale.applied_ids.is_empty());
    assert_eq!(
        messages.counts(None, None).unwrap().pending,
        3,
        "a stale count changes nothing"
    );

    let applied = messages
        .apply_action_scope(InboxAction::Dismiss, &scope, 3)
        .unwrap();
    assert_eq!(applied.applied_ids.len(), 3);
    assert_eq!(applied.stale_count, None);
    assert_eq!(messages.counts(None, None).unwrap().dismissed, 3);

    // 筛选到这个分类已经没有行了：确认时说是 3 条，实际 0 条，仍然一条都不改。
    let empty_scope = InboxScope {
        category: Some("mail".into()),
        ..InboxScope::default()
    };
    let empty = messages
        .apply_action_scope(InboxAction::Read, &empty_scope, 3)
        .unwrap();
    assert_eq!(empty.stale_count, Some(0));
}

/// 老用户的库停在 v5：打开时必须补上 0006 / 0007，而不是要求重建。
#[test]
fn opening_a_v5_database_applies_the_task_and_inbox_migrations() {
    let temp_dir = std::env::temp_dir().join(format!("tomatodesk-v5-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let database_path = temp_dir.join("tomatodesk.sqlite3");

    {
        let connection = rusqlite::Connection::open(&database_path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE schema_migrations (
                     version INTEGER PRIMARY KEY,
                     name TEXT NOT NULL,
                     applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                 );",
            )
            .unwrap();
        for sql in [
            include_str!("../../../migrations/0001_initial.sql"),
            include_str!("../../../migrations/0002_item_body.sql"),
            include_str!("../../../migrations/0003_repo_workspace.sql"),
            include_str!("../../../migrations/0004_thread_comments.sql"),
            include_str!("../../../migrations/0005_capture_module.sql"),
        ] {
            connection.execute_batch(sql).unwrap();
        }
        for (version, name) in [
            (1, "initial_schema"),
            (2, "item_body"),
            (3, "repo_workspace"),
            (4, "thread_comments"),
            (5, "capture_module"),
        ] {
            connection
                .execute(
                    "INSERT INTO schema_migrations(version, name) VALUES (?1, ?2)",
                    (version, name),
                )
                .unwrap();
        }
    }

    let database = Database::open(&database_path).unwrap();
    assert_eq!(
        database.migrated_to(),
        Some(9),
        "opening a v5 database lands on the latest version"
    );
    assert_eq!(database.tasks().counts().unwrap().all, 0);
    assert!(database.inbox_sources().list().unwrap().is_empty());
    let created = database
        .tasks()
        .create(new_task("升级后的第一条待办", Some("manual"), None))
        .unwrap();
    assert_eq!(created.status, TaskStatus::Open);
    drop(database);

    let reopened = Database::open(&database_path).unwrap();
    assert_eq!(reopened.migrated_to(), None, "nothing left to migrate");
    assert_eq!(reopened.tasks().counts().unwrap().all, 1);
    drop(reopened);
    std::fs::remove_dir_all(temp_dir).unwrap();
}

#[test]
fn github_inbox_source_tracks_cursor_and_lookup_helpers_find_local_rows() {
    let db = Database::in_memory().unwrap();
    let sources = db.inbox_sources();
    let source = sources.ensure_github().unwrap();
    assert_eq!(source.kind, InboxSourceKind::Github);
    assert_eq!(sources.ensure_github().unwrap().id, source.id);
    assert_eq!(sources.list().unwrap().len(), 1, "github source is reused");

    assert_eq!(sources.cursor(&source.id).unwrap(), None);
    sources
        .mark_failed(&source.id, "GitHub Token 缺少 notifications 权限")
        .unwrap();
    assert_eq!(
        sources
            .get(&source.id)
            .unwrap()
            .unwrap()
            .last_error
            .as_deref(),
        Some("GitHub Token 缺少 notifications 权限")
    );

    sources
        .mark_synced(
            &source.id,
            Some("2026-11-26T10:00:00Z"),
            "2026-11-26T10:00:01Z",
        )
        .unwrap();
    let synced = sources.get(&source.id).unwrap().unwrap();
    assert_eq!(
        synced.last_error, None,
        "a successful sync clears the error"
    );
    assert_eq!(
        synced.last_synced_at.as_deref(),
        Some("2026-11-26T10:00:01Z")
    );
    assert_eq!(
        sources.cursor(&source.id).unwrap().as_deref(),
        Some("2026-11-26T10:00:00Z")
    );

    // 失败不清游标：下一次仍从上次成功的位置继续。
    sources.mark_failed(&source.id, "限流").unwrap();
    assert_eq!(
        sources.cursor(&source.id).unwrap().as_deref(),
        Some("2026-11-26T10:00:00Z")
    );

    let repo = db.github_repos().create("TencentCloud", "Octop").unwrap();
    let mut updated = repo.clone();
    updated.last_synced_at = Some("2026-11-26T00:00:00Z".into());
    db.github_repos()
        .apply_sync(&updated, &[thread(&repo.id, 482, ThreadState::Open)], &[])
        .unwrap();

    assert_eq!(
        db.github_repos()
            .find_id("tencentcloud", "octop")
            .unwrap()
            .as_deref(),
        Some(repo.id.as_str()),
        "owner and name match case insensitively"
    );
    assert_eq!(db.github_repos().find_id("other", "octop").unwrap(), None);

    let thread_id = db
        .github_threads()
        .find_id(&repo.id, 482)
        .unwrap()
        .expect("synced thread is found by number");
    assert_eq!(db.github_threads().find_id(&repo.id, 999).unwrap(), None);
    assert!(db.github_threads().get(&thread_id).unwrap().is_some());
}

#[test]
fn task_projection_resolves_the_inbox_source_kind_without_a_second_query() {
    let db = Database::in_memory().unwrap();
    let github = db.inbox_sources().ensure_github().unwrap();
    let message = db
        .inbox_messages()
        .ingest(NewInboxMessage {
            source_id: github.id.clone(),
            ..inbox_message(
                "notice-projection",
                "来自通知的待办",
                "2026-11-26T10:00:00Z",
            )
        })
        .unwrap();

    let tasks = db.tasks();
    let from_inbox = tasks
        .create(new_task("来自通知的待办", Some("inbox"), Some(&message.id)))
        .unwrap();
    assert_eq!(from_inbox.origin_source_kind.as_deref(), Some("github"));

    let manual = tasks
        .create(new_task("手动建的", Some("manual"), None))
        .unwrap();
    assert_eq!(
        manual.origin_source_kind, None,
        "a manual task has no inbox source to resolve"
    );

    // 列表投影与详情投影共用 TASK_FIELDS，这里确认分页查询也带上来源类型。
    let page = tasks
        .page(&TaskPageQuery {
            status: None,
            cursor: None,
            limit: 10,
        })
        .unwrap();
    let listed = page
        .tasks
        .iter()
        .find(|task| task.id == from_inbox.id)
        .expect("the converted task is in the page");
    assert_eq!(listed.origin_source_kind.as_deref(), Some("github"));
}

#[test]
fn read_back_queue_retries_then_blocks_and_follows_token_changes() {
    let db = Database::in_memory().unwrap();
    let source = db.inbox_sources().ensure_github().unwrap();
    let messages = db.inbox_messages();
    let message = messages
        .ingest(NewInboxMessage {
            source_id: source.id.clone(),
            ..inbox_message("9001", "通知", "2026-11-26T10:00:00Z")
        })
        .unwrap();

    let queue = db.inbox_read_back();
    queue.enqueue(&message.id, &source.id, "9001").unwrap();
    queue.enqueue(&message.id, &source.id, "9001").unwrap();
    let due = queue.due("2999-01-01T00:00:00Z", 10).unwrap();
    assert_eq!(due.len(), 1, "repeated reads reuse the same queue row");
    assert_eq!(due[0].attempts, 0);
    assert!(
        queue.due("2000-01-01T00:00:00Z", 10).unwrap().is_empty(),
        "rows scheduled in the future are not due yet"
    );

    queue
        .mark_retry(
            &due[0].id,
            1,
            "2026-11-26T10:05:00Z",
            "网络超时",
            ReadBackErrorKind::Transient,
        )
        .unwrap();
    assert!(queue.due("2026-11-26T10:00:00Z", 10).unwrap().is_empty());
    let due = queue.due("2026-11-26T10:06:00Z", 10).unwrap();
    assert_eq!(due[0].attempts, 1);
    assert_eq!(due[0].last_error.as_deref(), Some("网络超时"));
    assert_eq!(due[0].last_error_kind, Some(ReadBackErrorKind::Transient));

    // 认证失败转 blocked，不再自动到期。
    queue
        .mark_blocked(&due[0].id, 2, "Token 无效", ReadBackErrorKind::Auth)
        .unwrap();
    let status = queue.status().unwrap();
    assert_eq!((status.pending, status.blocked), (0, 1));
    assert_eq!(status.last_error.as_deref(), Some("Token 无效"));

    // 重新保存 Token：blocked 回到 pending 并把退避归零。
    assert_eq!(queue.reset_blocked().unwrap(), 1);
    let status = queue.status().unwrap();
    assert_eq!((status.pending, status.blocked), (1, 0));
    assert_eq!(status.last_error, None);

    // 同步把消息重新变成未读时，旧的已读意图作废。
    messages.set_read(&message.id, false).unwrap();
    assert_eq!(queue.remove_unread().unwrap(), 1);
    assert_eq!(queue.status().unwrap().pending, 0);

    // 删除 Token 丢弃 pending，保留 blocked 供「重新授权」提示。
    queue.enqueue(&message.id, &source.id, "9001").unwrap();
    assert_eq!(queue.clear_pending().unwrap(), 1);
    queue.enqueue(&message.id, &source.id, "9001").unwrap();
    let id = queue.due("2999-01-01T00:00:00Z", 10).unwrap()[0].id.clone();
    queue
        .mark_blocked(&id, 3, "缺少 notifications 权限", ReadBackErrorKind::Auth)
        .unwrap();
    assert_eq!(queue.clear_pending().unwrap(), 0);
    assert_eq!(queue.status().unwrap().blocked, 1);

    // 写入成功后删行，队列回到空。
    queue.remove(&id).unwrap();
    let status = queue.status().unwrap();
    assert_eq!((status.pending, status.blocked), (0, 0));
    assert_eq!(status.last_error, None);
}

#[test]
fn email_accounts_round_trip_config_and_survive_soft_delete() {
    let db = Database::in_memory().unwrap();
    let sources = db.inbox_sources();
    let id = account_id("me@qq.com", "imap.qq.com");
    let config = EmailConfig::new("me@qq.com", ("imap.qq.com", 993, EmailTls::Implicit), None);

    let created = sources
        .upsert_email(&id, "me@qq.com", &config.to_json())
        .unwrap();
    assert_eq!(created.kind, InboxSourceKind::Email);
    assert!(created.enabled);
    assert_eq!(sources.count_configured_email().unwrap(), 1);

    // 幂等：同一 id 再保存只更新配置，不新增行，created_at 不变。
    let updated = EmailConfig::new(
        "me@qq.com",
        ("imap.qq.com", 143, EmailTls::Starttls),
        Some("Archive"),
    );
    let again = sources
        .upsert_email(&id, "me@qq.com", &updated.to_json())
        .unwrap();
    assert_eq!(again.created_at, created.created_at);
    let rows = sources.list_email().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(EmailConfig::parse(&rows[0].config_json).unwrap(), updated);
    assert_eq!(
        sources.config(&id).unwrap().as_deref(),
        Some(updated.to_json().as_str())
    );

    // 游标：写入后能读回，显式清空与「重新保存配置」都会归零。
    sources
        .mark_synced(
            &id,
            Some(r#"{"uidValidity":1,"lastUid":42}"#),
            "2026-11-26T12:00:00Z",
        )
        .unwrap();
    assert!(sources.cursor(&id).unwrap().is_some());
    sources.clear_cursor(&id).unwrap();
    assert_eq!(sources.cursor(&id).unwrap(), None);

    sources
        .mark_synced(
            &id,
            Some(r#"{"uidValidity":1,"lastUid":42}"#),
            "2026-11-26T12:00:00Z",
        )
        .unwrap();
    sources
        .upsert_email(&id, "me@qq.com", &updated.to_json())
        .unwrap();
    assert_eq!(sources.cursor(&id).unwrap(), None);

    // 软停用：不占名额，但来源行与历史消息都保留。
    let message = db
        .inbox_messages()
        .ingest(NewInboxMessage {
            source_id: id.clone(),
            ..inbox_message("mail-1", "一封邮件", "2026-11-26T10:00:00Z")
        })
        .unwrap();
    sources.set_enabled(&id, false).unwrap();
    assert_eq!(sources.count_configured_email().unwrap(), 0);
    assert!(db.inbox_messages().get(&message.id).unwrap().is_some());
    assert!(!sources.list_email().unwrap()[0].source.enabled);

    // 重新保存同一账户等于重新启用：回到 enabled=1 且游标为空。
    let reenabled = sources
        .upsert_email(&id, "me@qq.com", &updated.to_json())
        .unwrap();
    assert!(reenabled.enabled);
    assert_eq!(sources.count_configured_email().unwrap(), 1);

    // 多账户：不同主机派生出不同来源，互不覆盖。
    let other = account_id("me@163.com", "imap.163.com");
    let other_config = EmailConfig::new(
        "me@163.com",
        ("imap.163.com", 993, EmailTls::Implicit),
        None,
    );
    sources
        .upsert_email(&other, "me@163.com", &other_config.to_json())
        .unwrap();
    assert_eq!(sources.count_configured_email().unwrap(), 2);
    assert_eq!(sources.list_email().unwrap().len(), 2);
    assert_eq!(
        EmailConfig::parse(&sources.config(&other).unwrap().unwrap()),
        Some(other_config)
    );
}
