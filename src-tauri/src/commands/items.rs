use tauri::State;

use crate::{
    domain::{Item, ItemStatus, NewItem},
    infrastructure::db::Database,
};

use super::{
    dto::{CreateItemInput, ItemIdInput, SearchItemsInput, UpdateItemInput, parse_input},
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

pub(super) fn list_items_in(database: &Database) -> CommandResult<Vec<Item>> {
    database.items().list().map_err(Into::into)
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
) -> CommandResult<Vec<Item>> {
    let input = input.validate()?;
    database.items().search(&input.query).map_err(Into::into)
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
pub fn list_items(database: State<'_, Database>) -> CommandResult<Vec<Item>> {
    list_items_in(&database)
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
) -> CommandResult<Vec<Item>> {
    search_items_in(&database, parse_input(input)?)
}
