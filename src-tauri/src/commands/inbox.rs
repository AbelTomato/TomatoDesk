use tauri::State;

use crate::{
    application,
    domain::{
        InboxActionResult, InboxCounts, InboxMessage, InboxPage, InboxReadBackStatus, InboxRule,
        InboxRulePreview, InboxSource, ReadBackAction, Task,
    },
    infrastructure::{db::Database, secrets},
};

use super::{
    dto::{
        ApplyInboxActionInput, CountInboxMessagesInput, IdInput, InboxActionTarget,
        InboxRuleFilterInput, IngestInboxMessageInput, ListInboxMessagesInput, SaveInboxRuleInput,
        UndoInboxActionInput, parse_input, parse_optional_input,
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

/// 批量动作：勾选与「选择全部 N 条」走同一个命令，返回真正改动的 id 供撤销。
pub(super) fn apply_inbox_action_in(
    database: &Database,
    input: ApplyInboxActionInput,
) -> CommandResult<InboxActionResult> {
    let (action, target) = input.validate()?;
    match target {
        InboxActionTarget::Ids(ids) => application::inbox::apply_action(database, action, &ids),
        InboxActionTarget::Scope { scope, expected } => {
            application::inbox::apply_action_scope(database, action, &scope, expected)
        }
    }
    .map_err(Into::into)
}

/// 撤销：前端把上一次返回的 `appliedIds` / `taskIds` 原样带回来，后端不留会话。
pub(super) fn undo_inbox_action_in(
    database: &Database,
    input: UndoInboxActionInput,
) -> CommandResult<InboxActionResult> {
    let (action, ids, task_ids) = input.validate()?;
    application::inbox::undo_action(database, action, &ids, &task_ids).map_err(Into::into)
}

/// 详情页的单条动作只是批量动作的一层薄封装。
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
/// 命中的规则会在这里就生效，返回值是规则处理之后的最终状态。
pub(super) fn ingest_inbox_message_in(
    database: &Database,
    input: IngestInboxMessageInput,
) -> CommandResult<InboxMessage> {
    let source = database
        .inbox_sources()
        .ensure_manual()
        .map_err(CommandError::from)?;
    let message = input.validate(source.id)?;
    application::inbox::ingest(database, message)
        .map(|outcome| outcome.message)
        .map_err(Into::into)
}

pub(super) fn get_inbox_read_back_status_in(
    database: &Database,
) -> CommandResult<InboxReadBackStatus> {
    database
        .inbox_read_back()
        .status()
        .map_err(CommandError::from)
}

/// 设置页的规则列表：按创建顺序，禁用的一起返回（列表里要能看到并重新打开）。
pub(super) fn list_inbox_rules_in(database: &Database) -> CommandResult<Vec<InboxRule>> {
    database.inbox_rules().list().map_err(Into::into)
}

pub(super) fn save_inbox_rule_in(
    database: &Database,
    input: SaveInboxRuleInput,
) -> CommandResult<InboxRule> {
    let form = input.validate()?;
    database.inbox_rules().upsert(form).map_err(Into::into)
}

/// 删除后直接回剩余列表：前端不必再发一次请求。
pub(super) fn delete_inbox_rule_in(
    database: &Database,
    input: IdInput,
) -> CommandResult<Vec<InboxRule>> {
    let input = input.validate()?;
    if !database
        .inbox_rules()
        .delete(&input.id)
        .map_err(CommandError::from)?
    {
        return Err(CommandError::not_found("inbox rule"));
    }
    database.inbox_rules().list().map_err(Into::into)
}

/// 试跑：只统计最近 14 天还没处理的消息，不改任何数据。
pub(super) fn preview_inbox_rule_in(
    database: &Database,
    input: InboxRuleFilterInput,
) -> CommandResult<InboxRulePreview> {
    let filter = input.into_filter()?;
    database.inbox_rules().preview(&filter).map_err(Into::into)
}

/// 手动重试：把 blocked 行放回队列。
///
/// 只有存在 GitHub 的 blocked 行时才要求配 Token：邮件行用的是各自的授权码，
/// 没有 GitHub Token 也不该拦住用户重试邮件。
pub(super) fn retry_inbox_read_back_in(database: &Database) -> CommandResult<InboxReadBackStatus> {
    let github_blocked = database
        .inbox_read_back()
        .has_blocked(ReadBackAction::GithubRead)
        .map_err(CommandError::from)?;
    if github_blocked
        && secrets::get_github_token()
            .map_err(CommandError::internal)?
            .is_none()
    {
        return Err(CommandError::validation("请先保存 GitHub Token"));
    }
    application::inbox::read_back::reset_after_token_saved(database).map_err(CommandError::from)?;
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

/// 批量动作：`ids` 与 `scope` 二选一，`scope` 需带 `expectedCount` 对账。
#[tauri::command]
pub fn apply_inbox_action(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<InboxActionResult> {
    apply_inbox_action_in(&database, parse_input(input)?)
}

/// 撤销上一次动作：8 秒窗口由前端计时，后端只按带回来的 id 反向执行。
#[tauri::command]
pub fn undo_inbox_action(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<InboxActionResult> {
    undo_inbox_action_in(&database, parse_input(input)?)
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

#[tauri::command]
pub fn list_inbox_rules(database: State<'_, Database>) -> CommandResult<Vec<InboxRule>> {
    list_inbox_rules_in(&database)
}

/// 保存规则：没有 `id` 是新建，有 `id` 是更新。空条件在这里被拒绝。
#[tauri::command]
pub fn save_inbox_rule(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<InboxRule> {
    save_inbox_rule_in(&database, parse_input(input)?)
}

#[tauri::command]
pub fn delete_inbox_rule(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<Vec<InboxRule>> {
    delete_inbox_rule_in(&database, parse_input(input)?)
}

/// 试跑：只看最近 14 天还有多少条会被命中，不写任何数据。
#[tauri::command]
pub fn preview_inbox_rule(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<InboxRulePreview> {
    preview_inbox_rule_in(&database, parse_optional_input(input)?)
}
