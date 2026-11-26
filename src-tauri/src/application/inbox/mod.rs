//! 收件箱应用服务：入库、已读、忽略与转为待办。
//!
//! 判断动作全部是人工触发，收件箱不会自动流入待办或随手记。
//! 连接器只负责入库，不碰判断结果。

pub(crate) mod github;

use crate::{
    domain::{InboxMessage, NewInboxMessage, NewTask, Task, TriageOutcome},
    infrastructure::db::{Database, DbError, DbResult},
};

/// 入库前先保证来源存在；开发用 ingest 固定复用 `source-manual`。
pub(crate) fn ingest(database: &Database, message: NewInboxMessage) -> DbResult<InboxMessage> {
    database.inbox_messages().ingest(message)
}

pub(crate) fn mark_read(
    database: &Database,
    id: &str,
    read: bool,
) -> DbResult<Option<InboxMessage>> {
    database.inbox_messages().set_read(id, read)
}

/// 忽略：软删除，重复点击幂等。
pub(crate) fn dismiss(database: &Database, id: &str) -> DbResult<Option<InboxMessage>> {
    database.inbox_messages().dismiss(id)
}

/// 转为待办。
///
/// 已经转过的消息直接返回原来那条待办，重复点击不会生成第二条；
/// 已经忽略或已提取为随手记的消息不再转换，避免判断结果被覆盖。
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

    database.inbox_messages().to_task(
        &message.id,
        NewTask {
            title: message.title.clone(),
            note: task_note(&message),
            origin_kind: Some("inbox".into()),
            origin_id: Some(message.id.clone()),
        },
    )
}

/// 待办备注取摘要与 URL：长正文留在收件箱，需要时回到原消息查看。
fn task_note(message: &InboxMessage) -> Option<String> {
    let summary = message
        .summary
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let url = message
        .url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    match (summary, url) {
        (Some(summary), Some(url)) => Some(format!("{summary}\n\n{url}")),
        (Some(summary), None) => Some(summary.to_owned()),
        (None, Some(url)) => Some(url.to_owned()),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::task_note;
    use crate::domain::InboxMessage;

    fn message(summary: Option<&str>, url: Option<&str>) -> InboxMessage {
        InboxMessage {
            id: "message".into(),
            source_id: "source".into(),
            external_id: "external".into(),
            category: "mention".into(),
            title: "标题".into(),
            summary: summary.map(ToOwned::to_owned),
            body: None,
            url: url.map(ToOwned::to_owned),
            actor: None,
            occurred_at: "2026-11-26T00:00:00.000Z".into(),
            read_at: None,
            triaged_at: None,
            triage_outcome: None,
            triage_ref_id: None,
            related_kind: None,
            related_id: None,
        }
    }

    #[test]
    fn task_notes_keep_summary_and_url_but_skip_blank_fields() {
        assert_eq!(
            task_note(&message(Some(" 摘要 "), Some("https://example.com"))).as_deref(),
            Some("摘要\n\nhttps://example.com")
        );
        assert_eq!(
            task_note(&message(Some("摘要"), None)).as_deref(),
            Some("摘要")
        );
        assert_eq!(
            task_note(&message(None, Some("https://example.com"))).as_deref(),
            Some("https://example.com")
        );
        assert_eq!(task_note(&message(Some("   "), None)), None);
        assert_eq!(task_note(&message(None, None)), None);
    }
}
