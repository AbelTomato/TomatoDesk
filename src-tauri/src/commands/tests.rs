use serde_json::json;

use crate::{
    commands::{
        dto::{
            CreateItemInput, CreateTagInput, ItemIdInput, SearchItemsInput, SetItemTagsInput,
            UpdateItemInput,
        },
        error::CommandErrorCode,
        items, tags,
    },
    domain::{ItemKind, ItemStatus},
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
    assert_eq!(
        items::list_items_in(&database).unwrap(),
        vec![created.clone()]
    );
    assert_eq!(
        items::search_items_in(
            &database,
            SearchItemsInput {
                query: "handbook".into()
            }
        )
        .unwrap(),
        vec![created.clone()]
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
