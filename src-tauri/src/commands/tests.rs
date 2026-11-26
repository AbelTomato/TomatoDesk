use serde_json::json;

use crate::{
    commands::{
        captures,
        dto::{
            CountCapturesInput, CreateCaptureInput, CreateTagInput, CreateTaskInput, IdInput,
            IngestInboxMessageInput, ListCapturesPageInput, ListInboxMessagesInput,
            ListRepoThreadsInput, ListTasksPageInput, SearchCapturesInput, SetCaptureTagsInput,
            UpdateCaptureInput, UpdateRepoMetaInput, UpdateTaskInput,
        },
        error::CommandErrorCode,
        inbox, repos, tags, tasks,
    },
    domain::{
        CaptureKind, CaptureSort, CaptureSummary, InboxSourceKind, TaskStatus, TriageOutcome,
    },
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

#[test]
fn task_commands_create_update_and_walk_the_status_flow() {
    let database = Database::in_memory().unwrap();
    let created = tasks::create_task_in(
        &database,
        CreateTaskInput {
            title: " 写阶段二测试 ".into(),
            note: Some("   ".into()),
        },
    )
    .unwrap();
    assert_eq!(created.title, "写阶段二测试");
    assert_eq!(created.note, None, "blank note is folded to None");
    assert_eq!(created.origin_kind.as_deref(), Some("manual"));
    assert_eq!(created.status, TaskStatus::Open);

    let updated = tasks::update_task_in(
        &database,
        UpdateTaskInput {
            id: created.id.clone(),
            title: "写接线测试".into(),
            note: Some("覆盖命令层".into()),
        },
    )
    .unwrap();
    assert_eq!(updated.title, "写接线测试");
    assert_eq!(updated.note.as_deref(), Some("覆盖命令层"));
    assert_eq!(updated.status, TaskStatus::Open, "editing keeps the status");

    let done = tasks::complete_task_in(
        &database,
        IdInput {
            id: created.id.clone(),
        },
    )
    .unwrap();
    assert_eq!(done.status, TaskStatus::Done);
    assert!(done.completed_at.is_some());

    let reopened = tasks::reopen_task_in(
        &database,
        IdInput {
            id: created.id.clone(),
        },
    )
    .unwrap();
    assert_eq!(reopened.status, TaskStatus::Open);
    assert_eq!(reopened.completed_at, None);

    let cancelled = tasks::cancel_task_in(
        &database,
        IdInput {
            id: created.id.clone(),
        },
    )
    .unwrap();
    assert_eq!(cancelled.status, TaskStatus::Cancelled);

    let counts = tasks::count_tasks_in(&database).unwrap();
    assert_eq!(
        (counts.all, counts.open, counts.done, counts.cancelled),
        (1, 0, 0, 1),
        "counts follow the current status"
    );

    let page = tasks::list_tasks_page_in(
        &database,
        ListTasksPageInput {
            status: Some(TaskStatus::Cancelled),
            cursor: None,
            limit: None,
        },
    )
    .unwrap();
    assert_eq!(page.tasks, vec![cancelled]);

    let error = tasks::update_task_in(
        &database,
        UpdateTaskInput {
            id: "missing".into(),
            title: "x".into(),
            note: None,
        },
    )
    .unwrap_err();
    assert_eq!(error.code, CommandErrorCode::NotFound);

    let error = tasks::create_task_in(
        &database,
        CreateTaskInput {
            title: "   ".into(),
            note: None,
        },
    )
    .unwrap_err();
    assert_eq!(error.code, CommandErrorCode::Validation);
    assert_eq!(error.message, "title cannot be empty");

    let error = tasks::list_tasks_page_in(
        &database,
        ListTasksPageInput {
            status: None,
            cursor: Some("not-a-cursor".into()),
            limit: None,
        },
    )
    .unwrap_err();
    assert_eq!(error.code, CommandErrorCode::Validation);

    let error = tasks::list_tasks_page_in(
        &database,
        ListTasksPageInput {
            status: None,
            cursor: None,
            limit: Some(0),
        },
    )
    .unwrap_err();
    assert_eq!(error.code, CommandErrorCode::Validation);
}

fn inbox_input(
    external_id: &str,
    category: &str,
    title: &str,
    hour: u32,
) -> IngestInboxMessageInput {
    let occurred_at = format!("2026-11-26T{hour:02}:00:00.000Z");
    IngestInboxMessageInput {
        external_id: external_id.into(),
        external_updated_at: Some(occurred_at.clone()),
        category: category.into(),
        title: title.into(),
        summary: Some("摘要".into()),
        body: None,
        url: Some(format!("https://example.com/{external_id}")),
        actor: Some("reviewer".into()),
        occurred_at: Some(occurred_at),
        related_kind: None,
        related_id: None,
        payload_json: None,
    }
}

#[test]
fn inbox_commands_ingest_triage_and_convert_exactly_once() {
    let database = Database::in_memory().unwrap();
    let mut ids = Vec::new();
    for (index, category) in ["mention", "ci_failed", "release"].iter().enumerate() {
        let message = inbox::ingest_inbox_message_in(
            &database,
            inbox_input(
                &format!("notice-{index}"),
                category,
                &format!("通知 {index}"),
                index as u32,
            ),
        )
        .unwrap();
        assert_eq!(message.read_at, None);
        assert_eq!(message.triaged_at, None);
        ids.push(message.id);
    }

    let counts = inbox::count_inbox_pending_in(&database).unwrap();
    assert_eq!(
        (counts.all, counts.pending, counts.unread),
        (3, 3, 3),
        "every ingested message starts pending and unread"
    );

    let sources = inbox::list_inbox_sources_in(&database).unwrap();
    assert_eq!(sources.len(), 1, "manual source is reused across ingests");
    assert_eq!(sources[0].kind, InboxSourceKind::Manual);

    let read =
        inbox::mark_inbox_message_read_in(&database, IdInput { id: ids[0].clone() }).unwrap();
    assert!(
        read.read_at.is_some(),
        "read stamps read_at, not triaged_at"
    );
    assert_eq!(read.triaged_at, None);
    assert_eq!(inbox::count_inbox_pending_in(&database).unwrap().unread, 2);

    let unread =
        inbox::mark_inbox_message_unread_in(&database, IdInput { id: ids[0].clone() }).unwrap();
    assert_eq!(unread.read_at, None);

    let page = inbox::list_inbox_messages_page_in(
        &database,
        ListInboxMessagesInput {
            triaged: Some(false),
            unread_only: true,
            category: None,
            source_id: None,
            cursor: None,
            limit: Some(2),
        },
    )
    .unwrap();
    assert_eq!(page.messages.len(), 2);
    assert!(
        page.has_more,
        "three messages do not fit in one page of two"
    );
    let next = inbox::list_inbox_messages_page_in(
        &database,
        ListInboxMessagesInput {
            triaged: Some(false),
            unread_only: true,
            category: None,
            source_id: None,
            cursor: page.next_cursor,
            limit: Some(2),
        },
    )
    .unwrap();
    assert_eq!(next.messages.len(), 1, "cursor walks to the last page");
    assert!(!next.has_more);

    let filtered = inbox::list_inbox_messages_page_in(
        &database,
        ListInboxMessagesInput {
            triaged: None,
            unread_only: false,
            category: Some("release".into()),
            source_id: None,
            cursor: None,
            limit: None,
        },
    )
    .unwrap();
    assert_eq!(filtered.messages.len(), 1, "category filter reaches SQL");

    let dismissed =
        inbox::dismiss_inbox_message_in(&database, IdInput { id: ids[0].clone() }).unwrap();
    assert_eq!(dismissed.triage_outcome, Some(TriageOutcome::Dismissed));
    assert!(dismissed.triaged_at.is_some());
    assert_eq!(inbox::count_inbox_pending_in(&database).unwrap().pending, 2);

    let task =
        inbox::convert_inbox_message_to_task_in(&database, IdInput { id: ids[1].clone() }).unwrap();
    assert_eq!(task.title, "通知 1");
    assert_eq!(task.origin_kind.as_deref(), Some("inbox"));
    assert_eq!(task.origin_id.as_deref(), Some(ids[1].as_str()));
    assert_eq!(
        task.origin_source_kind.as_deref(),
        Some("manual"),
        "the task carries the source kind so the list can name it without a lookup"
    );
    assert_eq!(
        task.note.as_deref(),
        Some("摘要\n\nhttps://example.com/notice-1"),
        "the task note carries the summary and the source link"
    );

    let triaged = inbox::list_inbox_messages_page_in(
        &database,
        ListInboxMessagesInput {
            triaged: Some(true),
            unread_only: false,
            category: None,
            source_id: None,
            cursor: None,
            limit: None,
        },
    )
    .unwrap();
    assert_eq!(
        triaged.messages.len(),
        2,
        "dismiss and convert both count as triaged"
    );

    let again =
        inbox::convert_inbox_message_to_task_in(&database, IdInput { id: ids[1].clone() }).unwrap();
    assert_eq!(again.id, task.id, "repeat conversion returns the same task");

    let error = inbox::convert_inbox_message_to_task_in(&database, IdInput { id: ids[0].clone() })
        .unwrap_err();
    assert_eq!(
        error.code,
        CommandErrorCode::Validation,
        "a dismissed message cannot be converted"
    );

    let revived = inbox::ingest_inbox_message_in(
        &database,
        inbox_input("notice-0", "mention", "通知 0（更新）", 5),
    )
    .unwrap();
    assert_eq!(
        revived.triage_outcome,
        Some(TriageOutcome::Dismissed),
        "re-ingest keeps the triage result"
    );

    let error = inbox::ingest_inbox_message_in(
        &database,
        IngestInboxMessageInput {
            title: "   ".into(),
            ..inbox_input("notice-blank", "mention", "占位", 6)
        },
    )
    .unwrap_err();
    assert_eq!(error.code, CommandErrorCode::Validation);
    assert_eq!(error.message, "title cannot be empty");

    let error = inbox::ingest_inbox_message_in(
        &database,
        IngestInboxMessageInput {
            payload_json: Some("{not json".into()),
            ..inbox_input("notice-payload", "mention", "坏 payload", 7)
        },
    )
    .unwrap_err();
    assert_eq!(error.code, CommandErrorCode::Validation);
    assert_eq!(error.message, "payloadJson must be valid JSON");
}

#[test]
fn get_inbox_message_returns_the_row_and_reports_missing_ids() {
    let database = Database::in_memory().unwrap();
    let created = inbox::ingest_inbox_message_in(
        &database,
        inbox_input("notice-get", "mention", "详情消息", 1),
    )
    .unwrap();

    let loaded = inbox::get_inbox_message_in(
        &database,
        IdInput {
            id: created.id.clone(),
        },
    )
    .expect("the message can be loaded by id");
    assert_eq!(loaded.id, created.id);
    assert_eq!(loaded.title, "详情消息");
    assert_eq!(loaded.source_id, created.source_id);

    let error = inbox::get_inbox_message_in(
        &database,
        IdInput {
            id: "missing".into(),
        },
    )
    .unwrap_err();
    assert_eq!(error.code, CommandErrorCode::NotFound);

    // 空 id 在校验层就被拒绝，不会落到 SQL。
    let error = inbox::get_inbox_message_in(&database, IdInput { id: "  ".into() }).unwrap_err();
    assert_eq!(error.code, CommandErrorCode::Validation);
}

#[test]
fn marking_a_notification_read_enqueues_the_write_back_and_unread_cancels_it() {
    let database = Database::in_memory().unwrap();
    let source = database.inbox_sources().ensure_github().unwrap();
    let message = database
        .inbox_messages()
        .ingest(crate::domain::NewInboxMessage {
            source_id: source.id.clone(),
            external_id: "4242".into(),
            external_updated_at: Some("2026-11-26T09:00:00Z".into()),
            category: "mention".into(),
            title: "评审请求".into(),
            summary: Some("TencentCloud/Octop · 请求你评审".into()),
            body: None,
            url: Some("https://github.com/TencentCloud/Octop/pull/482".into()),
            actor: None,
            occurred_at: Some("2026-11-26T09:00:00Z".into()),
            related_kind: None,
            related_id: None,
            payload_json: None,
        })
        .unwrap();

    let read = inbox::mark_inbox_message_read_in(
        &database,
        IdInput {
            id: message.id.clone(),
        },
    )
    .unwrap();
    assert!(read.read_at.is_some(), "the local read state is written");
    assert_eq!(
        inbox::get_inbox_read_back_status_in(&database)
            .unwrap()
            .pending,
        1,
        "a github notification is queued for the remote write"
    );

    // 改回未读会撤销还没写出去的意图，避免远端被标已读、本地却显示未读。
    inbox::mark_inbox_message_unread_in(
        &database,
        IdInput {
            id: message.id.clone(),
        },
    )
    .unwrap();
    let status = inbox::get_inbox_read_back_status_in(&database).unwrap();
    assert_eq!((status.pending, status.blocked), (0, 0));

    // 人工来源没有远端可写，不入队。
    let manual =
        inbox::ingest_inbox_message_in(&database, inbox_input("manual-1", "mention", "手动", 1))
            .unwrap();
    inbox::mark_inbox_message_read_in(&database, IdInput { id: manual.id }).unwrap();
    assert_eq!(
        inbox::get_inbox_read_back_status_in(&database)
            .unwrap()
            .pending,
        0
    );
}
