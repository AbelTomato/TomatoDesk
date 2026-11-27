use tauri::State;

use crate::{
    application,
    domain::{InboxCounts, InboxMessage, InboxPage, InboxReadBackStatus, InboxSource, Task},
    infrastructure::{db::Database, secrets},
};

use super::{
    dto::{
        CountInboxMessagesInput, IdInput, IngestInboxMessageInput, ListInboxMessagesInput,
        parse_input, parse_optional_input,
    },
    error::{CommandError, CommandResult},
};

pub(super) fn list_inbox_messages_page_in(
    database: &Database,
    input: ListInboxMessagesInput,
) -> CommandResult<InboxPage> {
    let query = input.into_query()?;
    database.inbox_messages().page(&query).map_err(Into::into)
}

/// 一次查询同时返回五个计数：导航角标取 `pending`，四个 Tab 各取自己的一项。
/// 只按来源与分类过滤，切 Tab 不会让数字变化。
pub(super) fn count_inbox_messages_in(
    database: &Database,
    input: CountInboxMessagesInput,
) -> CommandResult<InboxCounts> {
    let input = input.validate()?;
    database
        .inbox_messages()
        .counts(input.source_id.as_deref(), input.category.as_deref())
        .map_err(Into::into)
}

pub(super) fn list_inbox_sources_in(database: &Database) -> CommandResult<Vec<InboxSource>> {
    database.inbox_sources().list().map_err(Into::into)
}

pub(super) fn get_inbox_message_in(
    database: &Database,
    input: IdInput,
) -> CommandResult<InboxMessage> {
    let input = input.validate()?;
    database
        .inbox_messages()
        .get(&input.id)
        .map_err(CommandError::from)?
        .ok_or_else(|| CommandError::not_found("inbox message"))
}

fn change_read_in(database: &Database, input: IdInput, read: bool) -> CommandResult<InboxMessage> {
    let input = input.validate()?;
    application::inbox::mark_read(database, &input.id, read)
        .map_err(CommandError::from)?
        .ok_or_else(|| CommandError::not_found("inbox message"))
}

pub(super) fn mark_inbox_message_read_in(
    database: &Database,
    input: IdInput,
) -> CommandResult<InboxMessage> {
    change_read_in(database, input, true)
}

pub(super) fn mark_inbox_message_unread_in(
    database: &Database,
    input: IdInput,
) -> CommandResult<InboxMessage> {
    change_read_in(database, input, false)
}

pub(super) fn dismiss_inbox_message_in(
    database: &Database,
    input: IdInput,
) -> CommandResult<InboxMessage> {
    let input = input.validate()?;
    application::inbox::dismiss(database, &input.id)
        .map_err(CommandError::from)?
        .ok_or_else(|| CommandError::not_found("inbox message"))
}

pub(super) fn convert_inbox_message_to_task_in(
    database: &Database,
    input: IdInput,
) -> CommandResult<Task> {
    let input = input.validate()?;
    application::inbox::to_task(database, &input.id).map_err(CommandError::from)
}

/// 开发与测试用：先确保人工来源存在，再按 `(source_id, external_id)` 去重写入。
pub(super) fn ingest_inbox_message_in(
    database: &Database,
    input: IngestInboxMessageInput,
) -> CommandResult<InboxMessage> {
    let source = database
        .inbox_sources()
        .ensure_manual()
        .map_err(CommandError::from)?;
    let message = input.validate(source.id)?;
    application::inbox::ingest(database, message).map_err(Into::into)
}

pub(super) fn get_inbox_read_back_status_in(
    database: &Database,
) -> CommandResult<InboxReadBackStatus> {
    database
        .inbox_read_back()
        .status()
        .map_err(CommandError::from)
}

/// 手动重试：把 blocked 行放回队列。没有 Token 时先让用户去配，
/// 否则重置完也只会再失败一次。
pub(super) fn retry_inbox_read_back_in(database: &Database) -> CommandResult<InboxReadBackStatus> {
    if secrets::get_github_token()
        .map_err(CommandError::internal)?
        .is_none()
    {
        return Err(CommandError::validation("请先保存 GitHub Token"));
    }
    application::inbox::github::reset_after_token_saved(database).map_err(CommandError::from)?;
    get_inbox_read_back_status_in(database)
}

#[tauri::command]
pub fn list_inbox_messages_page(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<InboxPage> {
    list_inbox_messages_page_in(&database, parse_input(input)?)
}

#[tauri::command]
pub fn count_inbox_messages(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<InboxCounts> {
    count_inbox_messages_in(&database, parse_optional_input(input)?)
}

#[tauri::command]
pub fn list_inbox_sources(database: State<'_, Database>) -> CommandResult<Vec<InboxSource>> {
    list_inbox_sources_in(&database)
}

#[tauri::command]
pub fn get_inbox_message(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<InboxMessage> {
    get_inbox_message_in(&database, parse_input(input)?)
}

/// 已读只写本地并入队，网络动作交给后台回写循环，点已读不会被请求拖住。
#[tauri::command]
pub fn mark_inbox_message_read(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<InboxMessage> {
    mark_inbox_message_read_in(&database, parse_input(input)?)
}

#[tauri::command]
pub fn mark_inbox_message_unread(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<InboxMessage> {
    mark_inbox_message_unread_in(&database, parse_input(input)?)
}

#[tauri::command]
pub fn dismiss_inbox_message(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<InboxMessage> {
    dismiss_inbox_message_in(&database, parse_input(input)?)
}

#[tauri::command]
pub fn convert_inbox_message_to_task(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<Task> {
    convert_inbox_message_to_task_in(&database, parse_input(input)?)
}

/// 只在开发与测试中使用：正式构建直接拒绝，避免成为未受保护的数据入口。
#[tauri::command]
pub fn ingest_inbox_message(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<InboxMessage> {
    if !cfg!(debug_assertions) {
        return Err(CommandError::internal(
            "ingest is only available in development builds",
        ));
    }
    ingest_inbox_message_in(&database, parse_input(input)?)
}

#[tauri::command]
pub fn get_inbox_read_back_status(
    database: State<'_, Database>,
) -> CommandResult<InboxReadBackStatus> {
    get_inbox_read_back_status_in(&database)
}

#[tauri::command]
pub fn retry_inbox_read_back(database: State<'_, Database>) -> CommandResult<InboxReadBackStatus> {
    retry_inbox_read_back_in(&database)
}
