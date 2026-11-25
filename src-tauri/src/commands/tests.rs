use serde_json::json;

use crate::{
    commands::{
        dto::{
            CountItemsInput, CreateItemInput, CreateTagInput, ItemIdInput, ListItemsPageInput,
            ListRepoThreadsInput, SearchItemsInput, SetItemTagsInput, UpdateItemInput,
            UpdateRepoMetaInput,
        },
        error::CommandErrorCode,
        items, repos, tags,
    },
    domain::{ItemKind, ItemSort, ItemStatus, ItemSummary},
    infrastructure::db::Database,
};

#[test]
fn item_commands_support_create_list_search_update_and_archive() {
    let database = Database::in_memory().unwrap();
    let created = items::create_item_in(
        &database,
        CreateItemInput {
            kind: ItemKind::Note,
            source: " manual ".into(),
            url: None,
            title: Some("Rust handbook".into()),
            summary: None,
            note: Some("save this".into()),
            repo_id: None,
        },
    )
    .unwrap();

    assert_eq!(created.source, "manual");
    assert_eq!(
        items::get_item_in(
            &database,
            ItemIdInput {
                id: created.id.clone()
            }
        )
        .unwrap(),
        created
    );
    let page = items::list_items_page_in(
        &database,
        ListItemsPageInput {
            status: None,
            kind: None,
            repo_id: None,
            sort: None,
            cursor: None,
            limit: None,
        },
    )
    .unwrap();
    assert_eq!(page.items, vec![ItemSummary::from(&created)]);
    assert!(!page.has_more);
    assert_eq!(page.next_cursor, None);
    assert_eq!(
        items::search_items_in(
            &database,
            SearchItemsInput {
                query: "handbook".into()
            }
        )
        .unwrap(),
        vec![ItemSummary::from(&created)]
    );

    let updated = items::update_item_in(
        &database,
        UpdateItemInput {
            id: created.id.clone(),
            kind: ItemKind::Link,
            source: "clipboard".into(),
            url: Some("https://example.com".into()),
            title: Some("Example".into()),
            summary: None,
            note: None,
            status: ItemStatus::Processed,
        },
    )
    .unwrap();
    assert_eq!(updated.status, ItemStatus::Processed);
    assert!(
        items::search_items_in(
            &database,
            SearchItemsInput {
                query: "handbook".into()
            }
        )
        .unwrap()
        .is_empty()
    );

    let archived = items::archive_item_in(
        &database,
        ItemIdInput {
            id: created.id.clone(),
        },
    )
    .unwrap();
    assert_eq!(archived.status, ItemStatus::Archived);
}

#[test]
fn item_and_tag_commands_reject_invalid_input_with_a_stable_error_shape() {
    let database = Database::in_memory().unwrap();
    let error = items::create_item_in(
        &database,
        CreateItemInput {
            kind: ItemKind::Note,
            source: "  ".into(),
            url: None,
            title: None,
            summary: None,
            note: Some("note".into()),
            repo_id: None,
        },
    )
    .unwrap_err();

    assert_eq!(error.code, CommandErrorCode::Validation);
    assert_eq!(
        serde_json::to_value(error).unwrap(),
        json!({ "code": "validation", "message": "source cannot be empty" })
    );

    let error = tags::create_tag_in(
        &database,
        CreateTagInput {
            name: " ".into(),
            color: None,
        },
    )
    .unwrap_err();
    assert_eq!(error.code, CommandErrorCode::Validation);
}

#[test]
fn tag_commands_create_list_and_replace_item_tags() {
    let database = Database::in_memory().unwrap();
    let item = items::create_item_in(
        &database,
        CreateItemInput {
            kind: ItemKind::Note,
            source: "manual".into(),
            url: None,
            title: Some("Tagged item".into()),
            summary: None,
            note: None,
            repo_id: None,
        },
    )
    .unwrap();
    let tag = tags::create_tag_in(
        &database,
        CreateTagInput {
            name: " Reading ".into(),
            color: Some("#3b82f6".into()),
        },
    )
    .unwrap();

    assert_eq!(tag.name, "Reading");
    assert_eq!(tags::list_tags_in(&database).unwrap(), vec![tag.clone()]);
    assert_eq!(
        tags::set_item_tags_in(
            &database,
            SetItemTagsInput {
                item_id: item.id.clone(),
                tag_ids: vec![tag.id.clone(), tag.id.clone()]
            },
        )
        .unwrap(),
        vec![tag]
    );

    let error = tags::set_item_tags_in(
        &database,
        SetItemTagsInput {
            item_id: item.id,
            tag_ids: vec!["missing-tag".into()],
        },
    )
    .unwrap_err();
    assert_eq!(error.code, CommandErrorCode::NotFound);

    let error = tags::create_tag_in(
        &database,
        CreateTagInput {
            name: "reading".into(),
            color: None,
        },
    )
    .unwrap_err();
    assert_eq!(error.code, CommandErrorCode::Conflict);
}

#[test]
fn request_dtos_reject_unknown_fields_and_missing_items_use_not_found_errors() {
    assert!(
        serde_json::from_value::<CreateItemInput>(json!({
            "kind": "note",
            "source": "manual",
            "unexpected": true
        }))
        .is_err()
    );

    let error = super::dto::parse_input::<CreateItemInput>(Some(json!({
        "kind": "note",
        "source": "manual",
        "unexpected": true
    })))
    .unwrap_err();
    assert_eq!(error.code, CommandErrorCode::Validation);

    let database = Database::in_memory().unwrap();
    let error = items::get_item_in(
        &database,
        ItemIdInput {
            id: "missing".into(),
        },
    )
    .unwrap_err();
    assert_eq!(error.code, CommandErrorCode::NotFound);
}

#[test]
fn item_pages_walk_with_keyset_cursors_and_counts_match_the_rows() {
    let database = Database::in_memory().unwrap();
    let mut created_ids = Vec::new();
    for index in 0..5 {
        let created = items::create_item_in(
            &database,
            CreateItemInput {
                kind: ItemKind::Note,
                source: "manual".into(),
                url: None,
                title: Some(format!("Item {index}")),
                summary: None,
                note: None,
                repo_id: None,
            },
        )
        .unwrap();
        created_ids.push(created.id);
    }

    let mut collected = Vec::new();
    let mut cursor = None;
    let mut pages = 0;
    loop {
        let page = items::list_items_page_in(
            &database,
            ListItemsPageInput {
                status: None,
                kind: None,
                repo_id: None,
                sort: Some(ItemSort::Updated),
                cursor: cursor.clone(),
                limit: Some(2),
            },
        )
        .unwrap();
        pages += 1;
        collected.extend(page.items.into_iter().map(|item| item.id));
        match page.next_cursor {
            Some(next) if page.has_more => cursor = Some(next),
            _ => break,
        }
    }

    assert_eq!(pages, 3);
    assert_eq!(collected.len(), 5);
    let mut unique = collected.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 5);

    let counts = items::count_items_in(&database, CountItemsInput::default()).unwrap();
    assert_eq!(counts.all, 5);
    assert_eq!(counts.pending, 5);
    assert_eq!(counts.processed, 0);
    assert_eq!(counts.archived, 0);
    assert_eq!(
        created_ids.len(),
        counts.all as usize,
        "created rows and counted rows should agree"
    );
}

#[test]
fn item_pages_reject_invalid_cursors_and_out_of_range_limits() {
    let database = Database::in_memory().unwrap();

    let error = items::list_items_page_in(
        &database,
        ListItemsPageInput {
            status: None,
            kind: None,
            repo_id: None,
            sort: None,
            cursor: Some("not-a-cursor".into()),
            limit: None,
        },
    )
    .unwrap_err();
    assert_eq!(error.code, CommandErrorCode::Validation);

    for limit in [0, 201] {
        let error = items::list_items_page_in(
            &database,
            ListItemsPageInput {
                status: None,
                kind: None,
                repo_id: None,
                sort: None,
                cursor: None,
                limit: Some(limit),
            },
        )
        .unwrap_err();
        assert_eq!(error.code, CommandErrorCode::Validation);
    }
}

#[test]
fn repo_commands_expose_meta_and_reject_missing_threads() {
    let database = Database::in_memory().unwrap();
    let repo = database.github_repos().create("owner", "project").unwrap();

    let summaries = repos::list_repos_in(&database).unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].open_pull_requests, 0);
    assert_eq!(summaries[0].description, None);

    // 自定义描述优先展示，空字符串等于清除。
    let detail = repos::update_repo_meta_in(
        &database,
        UpdateRepoMetaInput {
            id: repo.id.clone(),
            custom_description: Some("  我的项目  ".into()),
            custom_note: Some("".into()),
        },
    )
    .unwrap();
    assert_eq!(
        detail.repo.custom_description.as_deref(),
        Some("我的项目"),
        "values are trimmed"
    );
    assert_eq!(detail.repo.custom_note, None, "blank clears the note");
    assert_eq!(
        repos::list_repos_in(&database).unwrap()[0]
            .description
            .as_deref(),
        Some("我的项目")
    );

    let page = repos::list_repo_threads_in(
        &database,
        ListRepoThreadsInput {
            repo_id: repo.id.clone(),
            kind: None,
            state: None,
            relationship: None,
            cursor: None,
            limit: None,
        },
    )
    .unwrap();
    assert!(page.threads.is_empty());
    assert!(!page.has_more);

    let error = repos::list_repo_threads_in(
        &database,
        ListRepoThreadsInput {
            repo_id: repo.id,
            kind: None,
            state: None,
            relationship: None,
            cursor: Some("broken".into()),
            limit: None,
        },
    )
    .unwrap_err();
    assert_eq!(error.code, CommandErrorCode::Validation);

    let error = repos::get_thread_in(
        &database,
        ItemIdInput {
            id: "missing".into(),
        },
    )
    .unwrap_err();
    assert_eq!(error.code, CommandErrorCode::NotFound);
}

#[test]
fn items_can_be_scoped_to_a_repository() {
    let database = Database::in_memory().unwrap();
    let repo = database.github_repos().create("owner", "project").unwrap();
    let created = items::create_item_in(
        &database,
        CreateItemInput {
            kind: ItemKind::Note,
            source: "repo".into(),
            url: None,
            title: Some("仓库笔记".into()),
            summary: None,
            note: Some("记一条".into()),
            repo_id: Some(repo.id.clone()),
        },
    )
    .unwrap();
    assert_eq!(created.repo_id.as_deref(), Some(repo.id.as_str()));

    let page = items::list_items_page_in(
        &database,
        ListItemsPageInput {
            status: None,
            kind: None,
            repo_id: Some(repo.id.clone()),
            sort: None,
            cursor: None,
            limit: None,
        },
    )
    .unwrap();
    assert_eq!(page.items, vec![ItemSummary::from(&created)]);

    let unscoped = items::list_items_page_in(
        &database,
        ListItemsPageInput {
            status: None,
            kind: None,
            repo_id: None,
            sort: None,
            cursor: None,
            limit: None,
        },
    )
    .unwrap();
    assert_eq!(
        unscoped.items.len(),
        1,
        "the inbox still lists repo notes as personal items"
    );

    let updated = items::update_item_in(
        &database,
        UpdateItemInput {
            id: created.id.clone(),
            kind: ItemKind::Note,
            source: "repo".into(),
            url: None,
            title: Some("仓库笔记".into()),
            summary: None,
            note: Some("改一条".into()),
            status: ItemStatus::Processed,
        },
    )
    .unwrap();
    assert_eq!(
        updated.repo_id.as_deref(),
        Some(repo.id.as_str()),
        "editing an item keeps its repository association"
    );
}
