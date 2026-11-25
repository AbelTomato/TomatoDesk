use super::Database;
use crate::domain::{
    GithubRelease, GithubThread, ItemKind, ItemPageQuery, ItemSort, ItemStatus, ItemSummary,
    KeysetCursor, NewItem, NewSyncRun, SyncRunStatus, ThreadKind, ThreadQuery, ThreadRelationship,
    ThreadState, ThreadSummary,
};
use uuid::Uuid;

fn item(title: &str, note: &str) -> NewItem {
    NewItem {
        kind: ItemKind::Note,
        source: "manual".into(),
        url: None,
        title: Some(title.into()),
        summary: None,
        note: Some(note.into()),
        body: None,
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
fn migrations_are_idempotent_and_items_support_crud() {
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
    assert_eq!(migration_count, 3);

    let items = db.items();
    let created = items.create(item("First title", "first note")).unwrap();
    assert_eq!(items.get(&created.id).unwrap(), Some(created.clone()));
    assert_eq!(items.list().unwrap(), vec![created.clone()]);

    let mut changed = created.clone();
    changed.title = Some("Updated title".into());
    items.update(&changed).unwrap();
    let updated = items.get(&created.id).unwrap().unwrap();
    assert_eq!(updated.title.as_deref(), Some("Updated title"));
    assert_eq!(updated.status, ItemStatus::Pending);

    assert!(items.delete(&created.id).unwrap());
    assert_eq!(items.get(&created.id).unwrap(), None);
    assert!(!items.delete(&created.id).unwrap());
}

#[test]
fn database_creates_parent_directories_and_persists_items() {
    let temp_dir = std::env::temp_dir().join(format!("tomatodesk-db-{}", Uuid::new_v4()));
    let database_path = temp_dir.join("nested/tomatodesk.sqlite3");
    let db = Database::open(&database_path).unwrap();
    assert_eq!(db.path(), Some(database_path.as_path()));
    let created = db
        .items()
        .create(item("Persistent", "survives reopen"))
        .unwrap();
    drop(db);

    let reopened = Database::open(&database_path).unwrap();
    assert_eq!(reopened.items().get(&created.id).unwrap(), Some(created));
    assert_eq!(
        reopened.migrated_to(),
        None,
        "a second open has nothing to migrate"
    );
    drop(reopened);
    std::fs::remove_dir_all(temp_dir).unwrap();
}

#[test]
fn fts_tracks_item_insert_update_and_delete() {
    let db = Database::in_memory().unwrap();
    let items = db.items();
    let created = items
        .create(item("Rust handbook", "keep this note searchable"))
        .unwrap();
    assert_eq!(items.search("handbook").unwrap().len(), 1);
    assert_eq!(items.search("searchable").unwrap().len(), 1);

    let mut changed = created.clone();
    changed.title = Some("SQLite guide".into());
    changed.note = Some("updated searchable content".into());
    items.update(&changed).unwrap();
    assert!(items.search("handbook").unwrap().is_empty());
    assert_eq!(items.search("SQLite").unwrap()[0].id, created.id);

    items.delete(&created.id).unwrap();
    assert!(items.search("SQLite").unwrap().is_empty());
}

#[test]
fn tags_and_item_tag_links_obey_foreign_keys() {
    let db = Database::in_memory().unwrap();
    let items = db.items();
    let tags = db.tags();
    let created = items.create(item("Tagged", "item")).unwrap();
    let tag = tags.create("Reading", Some("#3b82f6")).unwrap();

    tags.set_item_tags(&created.id, std::slice::from_ref(&tag.id))
        .unwrap();
    assert_eq!(tags.for_item(&created.id).unwrap(), vec![tag.clone()]);
    assert_eq!(tags.get(&tag.id).unwrap(), Some(tag.clone()));
    assert_eq!(tags.list().unwrap(), vec![tag.clone()]);
    assert!(
        tags.set_item_tags("missing-item", std::slice::from_ref(&tag.id))
            .is_err()
    );

    items.delete(&created.id).unwrap();
    assert!(tags.for_item(&created.id).unwrap().is_empty());
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

    // 个人条目可以挂到仓库下，并在详情里单独计数。
    let attached = db
        .items()
        .create(NewItem {
            repo_id: Some(repo.id.clone()),
            ..item("仓库笔记", "记一条")
        })
        .unwrap();
    assert_eq!(attached.repo_id.as_deref(), Some(repo.id.as_str()));
    let items = db
        .items()
        .page(&ItemPageQuery {
            status: None,
            kind: None,
            repo_id: Some(repo.id.clone()),
            sort: ItemSort::Updated,
            cursor: None,
            limit: 10,
        })
        .unwrap();
    assert_eq!(items.items, vec![ItemSummary::from(&attached)]);
    assert_eq!(repos.counts(&repo.id).unwrap().items, 1);

    assert_eq!(
        db.github_releases().list_for_repo(&repo.id).unwrap().len(),
        1
    );

    // 删除仓库会连带清掉线程与 Release，个人条目保留但解除关联。
    assert!(repos.delete(&repo.id).unwrap());
    assert!(db.github_threads().get(&first.id).unwrap().is_none());
    assert!(
        db.github_releases()
            .list_for_repo(&repo.id)
            .unwrap()
            .is_empty()
    );
    assert_eq!(db.items().get(&attached.id).unwrap().unwrap().repo_id, None);
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
