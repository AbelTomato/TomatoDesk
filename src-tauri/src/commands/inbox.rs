use tauri::State;

use crate::{
    application,
    domain::{InboxCounts, InboxMessage, InboxPage, InboxSource, Task},
    infrastructure::{db::Database, secrets},
};

use super::{
    dto::{IdInput, IngestInboxMessageInput, ListInboxMessagesInput, parse_input},
    error::{CommandError, CommandResult},
};

pub(super) fn list_inbox_messages_page_in(
    database: &Database,
    input: ListInboxMessagesInput,
) -> CommandResult<InboxPage> {
    let query = input.into_query()?;
    database.inbox_messages().page(&query).map_err(Into::into)
}

/// 名字沿用方案里的 `count_inbox_pending`；一次查询同时返回全部三个计数，
/// 导航角标取 `pending`，标签页取 `all`，不再多开命令。
pub(super) fn count_inbox_pending_in(database: &Database) -> CommandResult<InboxCounts> {
    database.inbox_messages().counts().map_err(Into::into)
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

#[tauri::command]
pub fn list_inbox_messages_page(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<InboxPage> {
    list_inbox_messages_page_in(&database, parse_input(input)?)
}

#[tauri::command]
pub fn count_inbox_pending(database: State<'_, Database>) -> CommandResult<InboxCounts> {
    count_inbox_pending_in(&database)
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

/// 本地标记已读后，把「已读」回写给 GitHub；回写失败不回滚本地状态，
/// 否则用户点了已读却被网络问题打回未读，体验更差。
#[tauri::command]
pub async fn mark_inbox_message_read(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<InboxMessage> {
    let input: IdInput = parse_input(input)?;
    let message = mark_inbox_message_read_in(&database, input)?;
    let _ = application::inbox::github::write_read_back(&database, &message).await;
    Ok(message)
}

/// 拉取 GitHub 通知。没有 Token 时直接给出可操作的提示，不做无意义的网络请求。
#[tauri::command]
pub async fn sync_inbox_github(
    database: State<'_, Database>,
) -> CommandResult<application::inbox::github::InboxSyncSummary> {
    if secrets::get_github_token()
        .map_err(CommandError::internal)?
        .is_none()
    {
        return Err(CommandError::validation("请先保存 GitHub Token"));
    }
    application::inbox::github::sync(&database)
        .await
        .map_err(CommandError::internal)
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
