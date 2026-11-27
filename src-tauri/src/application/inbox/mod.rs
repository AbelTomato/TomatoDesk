//! 收件箱应用服务：入库、规则、已读、忽略与转为待办。
//!
//! 处理动作默认全部由人触发；规则是唯一的例外，而且只对新入库的消息生效，
//! 只做「标为已读」与「忽略」，不会自动生成待办或随手记。

pub(crate) mod email;
pub(crate) mod github;
pub(crate) mod read_back;

use crate::{
    domain::{
        InboxAction, InboxActionResult, InboxMessage, InboxRule, InboxScope, IngestOutcome,
        NewInboxMessage, Task, TriageOutcome,
    },
    infrastructure::db::{Database, DbError, DbResult},
};

/// 入库前先保证来源存在；开发用 ingest 固定复用 `source-manual`。
///
/// 新建的消息立刻交给规则判断一次：命中第一条启用的规则就执行它的动作。
/// 重新同步的老消息（`created == false`）不跑规则 —— 它们是被人看过、
/// 甚至处理过的，规则再把结果改回去就变成「撤销之后又被收回去」。
pub(crate) fn ingest(database: &Database, message: NewInboxMessage) -> DbResult<IngestOutcome> {
    let mut outcome = database.inbox_messages().ingest(message)?;
    if outcome.created {
        apply_first_rule(database, &mut outcome)?;
    }
    Ok(outcome)
}

/// 按 `created_at` 顺序取第一条命中的规则并执行。
///
/// 命中条件为空、或动作没有真正改动任何行（例如消息已经被别的路径处理）时
/// 什么都不做：规则是「尽力而为」的自动处理，不该让入库失败。
fn apply_first_rule(database: &Database, outcome: &mut IngestOutcome) -> DbResult<()> {
    let Some(rule) = first_matching_rule(database, &outcome.message)? else {
        return Ok(());
    };
    let action = rule.action.to_action();
    let result = database
        .inbox_messages()
        .apply_rule(&rule.id, action, &outcome.message.id)?;
    if result.applied_ids.is_empty() {
        return Ok(());
    }
    // 规则标为已读等同于人工标已读：远端同步照常入队，不做第二套语义。
    sync_read_back(database, action, &result.applied_ids)?;
    if let Some(updated) = database.inbox_messages().get(&outcome.message.id)? {
        outcome.message = updated;
    }
    Ok(())
}

fn first_matching_rule(database: &Database, message: &InboxMessage) -> DbResult<Option<InboxRule>> {
    Ok(database
        .inbox_rules()
        .enabled()?
        .into_iter()
        .find(|rule| rule.matches(message)))
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
