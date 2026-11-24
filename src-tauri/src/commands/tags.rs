use tauri::State;

use crate::{domain::Tag, infrastructure::db::Database};

use super::{
    dto::{CreateTagInput, SetItemTagsInput, parse_input},
    error::{CommandError, CommandResult},
};

pub(super) fn list_tags_in(database: &Database) -> CommandResult<Vec<Tag>> {
    database.tags().list().map_err(Into::into)
}

pub(super) fn create_tag_in(database: &Database, input: CreateTagInput) -> CommandResult<Tag> {
    let input = input.validate()?;
    database
        .tags()
        .create(&input.name, input.color.as_deref())
        .map_err(Into::into)
}

pub(super) fn set_item_tags_in(
    database: &Database,
    input: SetItemTagsInput,
) -> CommandResult<Vec<Tag>> {
    let input = input.validate()?;
    if database
        .items()
        .get(&input.item_id)
        .map_err(CommandError::from)?
        .is_none()
    {
        return Err(CommandError::not_found("item"));
    }
    for tag_id in &input.tag_ids {
        if database
            .tags()
            .get(tag_id)
            .map_err(CommandError::from)?
            .is_none()
        {
            return Err(CommandError::not_found("tag"));
        }
    }
    database
        .tags()
        .set_item_tags(&input.item_id, &input.tag_ids)
        .map_err(CommandError::from)?;
    database.tags().for_item(&input.item_id).map_err(Into::into)
}

pub(super) fn get_item_tags_in(
    database: &Database,
    input: super::dto::ItemIdInput,
) -> CommandResult<Vec<Tag>> {
    let input = input.validate()?;
    if database
        .items()
        .get(&input.id)
        .map_err(CommandError::from)?
        .is_none()
    {
        return Err(CommandError::not_found("item"));
    }
    database.tags().for_item(&input.id).map_err(Into::into)
}

#[tauri::command]
pub fn list_tags(database: State<'_, Database>) -> CommandResult<Vec<Tag>> {
    list_tags_in(&database)
}

#[tauri::command]
pub fn create_tag(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<Tag> {
    create_tag_in(&database, parse_input(input)?)
}

#[tauri::command]
pub fn set_item_tags(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<Vec<Tag>> {
    set_item_tags_in(&database, parse_input(input)?)
}

#[tauri::command]
pub fn get_item_tags(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<Vec<Tag>> {
    get_item_tags_in(&database, parse_input(input)?)
}
