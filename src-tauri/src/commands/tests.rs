use serde_json::json;

use crate::{
    commands::{
        captures,
        dto::{
            CountCapturesInput, CreateCaptureInput, CreateTagInput, IdInput, ListCapturesPageInput,
            ListRepoThreadsInput, SearchCapturesInput, SetCaptureTagsInput, UpdateCaptureInput,
            UpdateRepoMetaInput,
        },
        error::CommandErrorCode,
        repos, tags,
    },
    domain::{CaptureKind, CaptureSort, CaptureSummary},
    infrastructure::db::Database,
};

#[test]
fn capture_commands_support_create_list_search_update_and_archive() {
    let database = Database::in_memory().unwrap();
    let created = captures::create_capture_in(
        &database,
        CreateCaptureInput {
            kind: CaptureKind::Note,
            source: " manual ".into(),
            url: None,
            title: Some("Rust handbook".into()),
            summary: None,
            note: None,
            body: Some("save this".into()),
            repo_id: None,
        },
    )
    .unwrap();

    assert_eq!(created.source, "manual");
    assert_eq!(created.body.as_deref(), Some("save this"));
    assert_eq!(
        captures::get_capture_in(
            &database,
            IdInput {
                id: created.id.clone()
            }
        )
        .unwrap(),
        created
    );
    let page = captures::list_captures_page_in(
        &database,
        ListCapturesPageInput {
            archived: None,
            kind: None,
            repo_id: None,
            sort: None,
            cursor: None,
            limit: None,
        },
    )
    .unwrap();
    assert_eq!(page.captures, vec![CaptureSummary::from(&created)]);
    assert!(!page.has_more);
    assert_eq!(page.next_cursor, None);
    assert_eq!(
        captures::search_captures_in(
            &database,
            SearchCapturesInput {
                query: "handbook".into()
            }
        )
        .unwrap(),
        vec![CaptureSummary::from(&created)]
    );

    let updated = captures::update_capture_in(
        &database,
        UpdateCaptureInput {
            id: created.id.clone(),
            kind: CaptureKind::Link,
            source: "quick_capture".into(),
            url: Some("https://example.com".into()),
            title: Some("Example".into()),
            summary: None,
            note: None,
        },
    )
    .unwrap();
    assert_eq!(updated.kind, CaptureKind::Link);
    assert!(
        captures::search_captures_in(
            &database,
            SearchCapturesInput {
                query: "handbook".into()
            }
        )
        .unwrap()
        .is_empty()
    );

    let archived = captures::archive_capture_in(
        &database,
        IdInput {
            id: created.id.clone(),
        },
    )
    .unwrap();
    assert!(archived.archived_at.is_some());
    assert!(
        captures::list_captures_page_in(
            &database,
            ListCapturesPageInput {
                archived: Some(false),
                kind: None,
                repo_id: None,
                sort: None,
                cursor: None,
                limit: None,
            },
        )
        .unwrap()
        .captures
        .is_empty()
    );
    assert_eq!(
        captures::list_captures_page_in(
            &database,
            ListCapturesPageInput {
                archived: Some(true),
                kind: None,
                repo_id: None,
                sort: None,
                cursor: None,
                limit: None,
            },
        )
        .unwrap()
        .captures
        .len(),
        1
    );

    let restored = captures::restore_capture_in(
        &database,
        IdInput {
            id: created.id.clone(),
        },
    )
    .unwrap();
    assert_eq!(restored.archived_at, None);
}

#[test]
fn capture_and_tag_commands_reject_invalid_input_with_a_stable_error_shape() {
    let database = Database::in_memory().unwrap();
    let error = captures::create_capture_in(
        &database,
        CreateCaptureInput {
            kind: CaptureKind::Note,
            source: "  ".into(),
            url: None,
            title: None,
            summary: None,
            note: None,
            body: Some("note".into()),
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
fn tag_commands_create_list_and_replace_capture_tags() {
    let database = Database::in_memory().unwrap();
    let capture = captures::create_capture_in(
        &database,
        CreateCaptureInput {
            kind: CaptureKind::Note,
            source: "manual".into(),
            url: None,
            title: Some("Tagged capture".into()),
            summary: None,
            note: None,
            body: Some("body".into()),
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
        tags::set_capture_tags_in(
            &database,
            SetCaptureTagsInput {
                capture_id: capture.id.clone(),
                tag_ids: vec![tag.id.clone(), tag.id.clone()]
            },
        )
        .unwrap(),
        vec![tag]
    );

    let error = tags::set_capture_tags_in(
        &database,
        SetCaptureTagsInput {
            capture_id: capture.id,
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
fn request_dtos_reject_unknown_fields_and_missing_captures_use_not_found_errors() {
    assert!(
        serde_json::from_value::<CreateCaptureInput>(json!({
            "kind": "note",
            "source": "manual",
            "unexpected": true
        }))
        .is_err()
    );

    let error = super::dto::parse_input::<CreateCaptureInput>(Some(json!({
        "kind": "note",
        "source": "manual",
        "unexpected": true
    })))
    .unwrap_err();
    assert_eq!(error.code, CommandErrorCode::Validation);

    let database = Database::in_memory().unwrap();
    let error = captures::get_capture_in(
        &database,
        IdInput {
            id: "missing".into(),
        },
    )
    .unwrap_err();
    assert_eq!(error.code, CommandErrorCode::NotFound);
}

#[test]
fn capture_pages_walk_with_keyset_cursors_and_counts_match_the_rows() {
    let database = Database::in_memory().unwrap();
    let mut created_ids = Vec::new();
    for index in 0..5 {
        let created = captures::create_capture_in(
            &database,
            CreateCaptureInput {
                kind: CaptureKind::Note,
                source: "manual".into(),
                url: None,
                title: Some(format!("Capture {index}")),
                summary: None,
                note: None,
                body: Some(format!("body {index}")),
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
        let page = captures::list_captures_page_in(
            &database,
            ListCapturesPageInput {
                archived: None,
                kind: None,
                repo_id: None,
                sort: Some(CaptureSort::Updated),
                cursor: cursor.clone(),
                limit: Some(2),
            },
        )
        .unwrap();
        pages += 1;
        collected.extend(page.captures.into_iter().map(|capture| capture.id));
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

    let counts = captures::count_captures_in(&database, CountCapturesInput::default()).unwrap();
    assert_eq!(counts.all, 5);
    assert_eq!(counts.active, 5);
    assert_eq!(counts.archived, 0);
    assert_eq!(
        created_ids.len(),
        counts.all as usize,
        "created rows and counted rows should agree"
    );
}

#[test]
fn capture_pages_reject_invalid_cursors_and_out_of_range_limits() {
    let database = Database::in_memory().unwrap();

    let error = captures::list_captures_page_in(
        &database,
        ListCapturesPageInput {
            archived: None,
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
        let error = captures::list_captures_page_in(
            &database,
            ListCapturesPageInput {
                archived: None,
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
        IdInput {
            id: "missing".into(),
        },
    )
    .unwrap_err();
    assert_eq!(error.code, CommandErrorCode::NotFound);
}

#[test]
fn captures_can_be_scoped_to_a_repository() {
    let database = Database::in_memory().unwrap();
    let repo = database.github_repos().create("owner", "project").unwrap();
    let created = captures::create_capture_in(
        &database,
        CreateCaptureInput {
            kind: CaptureKind::Note,
            source: "repo".into(),
            url: None,
            title: Some("仓库笔记".into()),
            summary: None,
            note: None,
            body: Some("记一条".into()),
            repo_id: Some(repo.id.clone()),
        },
    )
    .unwrap();
    assert_eq!(created.repo_id.as_deref(), Some(repo.id.as_str()));

    let page = captures::list_captures_page_in(
        &database,
        ListCapturesPageInput {
            archived: None,
            kind: None,
            repo_id: Some(repo.id.clone()),
            sort: None,
            cursor: None,
            limit: None,
        },
    )
    .unwrap();
    assert_eq!(page.captures, vec![CaptureSummary::from(&created)]);

    let unscoped = captures::list_captures_page_in(
        &database,
        ListCapturesPageInput {
            archived: None,
            kind: None,
            repo_id: None,
            sort: None,
            cursor: None,
            limit: None,
        },
    )
    .unwrap();
    assert_eq!(
        unscoped.captures.len(),
        1,
        "the capture list still includes repo notes"
    );

    let updated = captures::update_capture_in(
        &database,
        UpdateCaptureInput {
            id: created.id.clone(),
            kind: CaptureKind::Note,
            source: "repo".into(),
            url: None,
            title: Some("仓库笔记".into()),
            summary: None,
            note: Some("改一条".into()),
        },
    )
    .unwrap();
    assert_eq!(
        updated.repo_id.as_deref(),
        Some(repo.id.as_str()),
        "editing a capture keeps its repository association"
    );
}

