use reqwest::Url;
use tauri::State;

use crate::{
    domain::{
        Item, ItemBody, ItemKind, ItemPage, ItemStatus, ItemStatusCounts, ItemSummary, NewItem,
    },
    infrastructure::{db::Database, web_metadata::fetch_web_text},
};

use super::{
    dto::{
        CountItemsInput, CreateItemInput, ItemIdInput, ListItemsPageInput, SearchItemsInput,
        UpdateItemInput, parse_input,
    },
    error::{CommandError, CommandResult},
};

pub(super) fn create_item_in(database: &Database, input: CreateItemInput) -> CommandResult<Item> {
    let input = input.validate()?;
    database
        .items()
        .create(NewItem {
            kind: input.kind,
            source: input.source,
            url: input.url,
            title: input.title,
            summary: input.summary,
            note: input.note,
            body: None,
        })
        .map_err(Into::into)
}

pub(super) fn get_item_in(database: &Database, input: ItemIdInput) -> CommandResult<Item> {
    let input = input.validate()?;
    database
        .items()
        .get(&input.id)
        .map_err(CommandError::from)?
        .ok_or_else(|| CommandError::not_found("item"))
}

pub(super) fn list_items_page_in(
    database: &Database,
    input: ListItemsPageInput,
) -> CommandResult<ItemPage> {
    let query = input.into_query()?;
    database.items().page(&query).map_err(Into::into)
}

pub(super) fn count_items_in(
    database: &Database,
    input: CountItemsInput,
) -> CommandResult<ItemStatusCounts> {
    database
        .items()
        .count_by_status(input.kind)
        .map_err(Into::into)
}

pub(super) fn update_item_in(database: &Database, input: UpdateItemInput) -> CommandResult<Item> {
    let input = input.validate()?;
    let mut item = get_item_in(
        database,
        ItemIdInput {
            id: input.id.clone(),
        },
    )?;
    item.kind = input.kind;
    item.source = input.source;
    item.url = input.url;
    item.title = input.title;
    item.summary = input.summary;
    item.note = input.note;
    item.status = input.status;

    if !database.items().update(&item).map_err(CommandError::from)? {
        return Err(CommandError::not_found("item"));
    }
    get_item_in(database, ItemIdInput { id: item.id })
}

pub(super) fn archive_item_in(database: &Database, input: ItemIdInput) -> CommandResult<Item> {
    let mut item = get_item_in(database, input)?;
    item.status = ItemStatus::Archived;
    if !database.items().update(&item).map_err(CommandError::from)? {
        return Err(CommandError::not_found("item"));
    }
    get_item_in(database, ItemIdInput { id: item.id })
}

pub(super) fn search_items_in(
    database: &Database,
    input: SearchItemsInput,
) -> CommandResult<Vec<ItemSummary>> {
    let input = input.validate()?;
    database.items().search(&input.query).map_err(Into::into)
}

/// 详情正文按需加载：本地有就返回，链接类为空时回源抓一次并写回缓存，
/// GitHub 条目只返回本地缓存，不触发网络请求。
pub(super) async fn get_item_body_in(
    database: &Database,
    input: ItemIdInput,
) -> CommandResult<ItemBody> {
    let input = input.validate()?;
    let body = database
        .items()
        .get_body(&input.id)
        .map_err(CommandError::from)?
        .ok_or_else(|| CommandError::not_found("item"))?;

    if body
        .body
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return Ok(body);
    }
    if body.kind != ItemKind::Link {
        return Ok(body);
    }
    let Some(url) = body
        .url
        .as_deref()
        .and_then(|value| Url::parse(value).ok())
        .filter(|url| matches!(url.scheme(), "http" | "https"))
    else {
        return Ok(body);
    };
    let Some(text) = fetch_web_text(&url).await else {
        return Ok(body);
    };
    database
        .items()
        .set_body(&body.id, &text)
        .map_err(CommandError::from)?;
    Ok(ItemBody {
        body: Some(text),
        ..body
    })
}

#[tauri::command]
pub fn create_item(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<Item> {
    create_item_in(&database, parse_input(input)?)
}

#[tauri::command]
pub fn get_item(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<Item> {
    get_item_in(&database, parse_input(input)?)
}

#[tauri::command]
pub fn list_items_page(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<ItemPage> {
    list_items_page_in(&database, parse_input(input)?)
}

#[tauri::command]
pub fn count_items_by_status(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<ItemStatusCounts> {
    let input = match input {
        Some(value) => serde_json::from_value(value)
            .map_err(|_| CommandError::validation("input is invalid"))?,
        None => CountItemsInput::default(),
    };
    count_items_in(&database, input)
}

#[tauri::command]
pub async fn get_item_body(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<ItemBody> {
    get_item_body_in(&database, parse_input(input)?).await
}

#[tauri::command]
pub fn update_item(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<Item> {
    update_item_in(&database, parse_input(input)?)
}

#[tauri::command]
pub fn archive_item(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<Item> {
    archive_item_in(&database, parse_input(input)?)
}

#[tauri::command]
pub fn search_items(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<Vec<ItemSummary>> {
    search_items_in(&database, parse_input(input)?)
}
