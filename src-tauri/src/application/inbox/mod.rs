//! 收件箱应用服务：入库、已读、忽略与转为待办。
//!
//! 处理动作全部是人工触发，收件箱不会自动流入待办或随手记。
//! 连接器只负责入库，不碰处理结果。

pub(crate) mod email;
pub(crate) mod github;
pub(crate) mod read_back;

use crate::{
    domain::{
        InboxAction, InboxActionResult, InboxMessage, InboxScope, NewInboxMessage, Task,
        TriageOutcome,
    },
    infrastructure::db::{Database, DbError, DbResult},
};

/// 入库前先保证来源存在；开发用 ingest 固定复用 `source-manual`。
pub(crate) fn ingest(database: &Database, message: NewInboxMessage) -> DbResult<InboxMessage> {
    database.inbox_messages().ingest(message)
}

/// 按 id 批量执行一个动作，单条与批量走同一条路径。
///
/// 本地写入先提交，已读回写再入队：队列失败不回滚已读，与点一条时的行为一致。
pub(crate) fn apply_action(
    database: &Database,
    action: InboxAction,
    ids: &[String],
) -> DbResult<InboxActionResult> {
    let result = database.inbox_messages().apply_action(action, ids)?;
    sync_read_back(database, action, &result.applied_ids)?;
    Ok(result)
}

/// 「选择全部 N 条」：条数与确认时不一致时返回 `stale_count`，一条都不改。
pub(crate) fn apply_action_scope(
    database: &Database,
    action: InboxAction,
    scope: &InboxScope,
    expected: i64,
) -> DbResult<InboxActionResult> {
    let result = database
        .inbox_messages()
        .apply_action_scope(action, scope, expected)?;
    sync_read_back(database, action, &result.applied_ids)?;
    Ok(result)
}

/// 撤销一次动作：`action` 是当初执行的动作，由前端随 `applied_ids` 带回来。
pub(crate) fn undo_action(
    database: &Database,
    action: InboxAction,
    ids: &[String],
    task_ids: &[String],
) -> DbResult<InboxActionResult> {
    let result = database
        .inbox_messages()
        .undo_action(action, ids, task_ids)?;
    if let Some(reversed) = action.reversed() {
        sync_read_back(database, reversed, &result.applied_ids)?;
    }
    Ok(result)
}

/// 改已读状态。本地写入是唯一的事实来源，远端同步交给回写队列：
/// 标已读入队，改回未读则撤销还没写出去的意图。
pub(crate) fn mark_read(
    database: &Database,
    id: &str,
    read: bool,
) -> DbResult<Option<InboxMessage>> {
    let action = if read {
        InboxAction::Read
    } else {
        InboxAction::Unread
    };
    apply_action(database, action, &[id.to_owned()])?;
    database.inbox_messages().get(id)
}

/// 忽略：软删除，重复点击幂等。
pub(crate) fn dismiss(database: &Database, id: &str) -> DbResult<Option<InboxMessage>> {
    apply_action(database, InboxAction::Dismiss, &[id.to_owned()])?;
    database.inbox_messages().get(id)
}

/// 转为待办。
///
/// 已经转过的消息直接返回原来那条待办，重复点击不会生成第二条；
/// 已经忽略或已提取为随手记的消息不再转换，避免处理结果被覆盖。
pub(crate) fn to_task(database: &Database, id: &str) -> DbResult<Task> {
    let message = database
        .inbox_messages()
        .get(id)?
        .ok_or(DbError::NotFound("inbox message"))?;

    if message.triaged_at.is_some() {
        if let Some(ref_id) = message.triage_ref_id.as_deref()
            && message.triage_outcome == Some(TriageOutcome::Task)
            && let Some(task) = database.tasks().get(ref_id)?
        {
            return Ok(task);
        }
        return Err(DbError::InvalidData(
            "inbox message is already triaged".into(),
        ));
    }

    let result = apply_action(
        database,
        InboxAction::Task,
        std::slice::from_ref(&message.id),
    )?;
    let task_id = result
        .task_ids
        .first()
        .ok_or_else(|| DbError::InvalidData("inbox message is already triaged".into()))?;
    database
        .tasks()
        .get(task_id)?
        .ok_or(DbError::NotFound("task"))
}

/// 只对真正变了的消息动队列：`read` 入队（GitHub 或邮件，按来源分派），
/// `unread` 撤销还没写出去的意图。
fn sync_read_back(database: &Database, action: InboxAction, ids: &[String]) -> DbResult<()> {
    match action {
        InboxAction::Read => {
            for id in ids {
                if let Some(message) = database.inbox_messages().get(id)? {
                    read_back::enqueue(database, &message)?;
                }
            }
        }
        InboxAction::Unread => {
            for id in ids {
                read_back::cancel(database, id)?;
            }
        }
        InboxAction::Dismiss | InboxAction::Task => {}
    }
    Ok(())
}
