use super::Database;
use crate::domain::{
    GithubEventType, ItemKind, ItemStatus, NewGithubEvent, NewItem, NewSyncRun, SyncRunStatus,
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
    assert_eq!(migration_count, 2);

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
fn github_events_are_idempotent_and_sync_runs_can_finish() {
    let db = Database::in_memory().unwrap();
    let repos = db.github_repos();
    let mut repo = repos.create("owner", "project").unwrap();
    assert_eq!(repos.get(&repo.id).unwrap(), Some(repo.clone()));

    let events = db.github_events();
    let event = NewGithubEvent {
        repo_id: repo.id.clone(),
        source_event_id: "release-1".into(),
        event_type: GithubEventType::Release,
        payload_json: r#"{"name":"v1"}"#.into(),
        occurred_at: "2026-11-24T00:00:00Z".into(),
    };
    assert!(events.insert(&event).unwrap());
    assert!(!events.insert(&event).unwrap());
    assert_eq!(events.list_for_repo(&repo.id).unwrap().len(), 1);

    repo.enabled = false;
    repo.etag = Some("W/\"v1\"".into());
    assert!(repos.update(&repo).unwrap());
    assert_eq!(repos.get(&repo.id).unwrap().unwrap().etag, repo.etag);
    assert_eq!(repos.list().unwrap().len(), 1);

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

    assert!(repos.delete(&repo.id).unwrap());
    assert!(events.list_for_repo(&repo.id).unwrap().is_empty());
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
