use rusqlite::{OptionalExtension, Row, params};
use uuid::Uuid;

use crate::{
    domain::{InboxReadBack, InboxReadBackStatus, ReadBackAction, ReadBackErrorKind},
    infrastructure::db::{Database, DbResult},
};

use super::parse_value;

const READ_BACK_FIELDS: &str = "id, message_id, source_id, external_id, action, uid, uid_validity, \
                                status, attempts, next_attempt_at, last_error, last_error_kind";

type RawReadBack = (
    String,
    String,
    String,
    String,
    String,
    Option<i64>,
    Option<i64>,
    String,
    i64,
    String,
    Option<String>,
    Option<String>,
);

/// 已读回写队列。
///
/// 只有远端写入失败时才留行，成功即删除，所以正常情况下这张表是空的；
/// 表里存在的每一行都表示「本地已读、远端还不知道」。
pub struct InboxReadBackRepository {
    database: Database,
}

impl InboxReadBackRepository {
    pub(crate) fn new(database: Database) -> Self {
        Self { database }
    }

    /// GitHub 通知标已读即入队：重复标已读会把退避归零，
    /// 用户的主动动作优先于上一次失败留下的等待时间。
    pub fn enqueue_github(
        &self,
        message_id: &str,
        source_id: &str,
        external_id: &str,
    ) -> DbResult<()> {
        self.database.with_connection(|connection| {
            connection.execute(
                "INSERT INTO inbox_read_back(
                     id, message_id, source_id, external_id, action, next_attempt_at
                 )
                 VALUES (?1, ?2, ?3, ?4, 'github_read',
                         strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                 ON CONFLICT(source_id, external_id) DO UPDATE SET
                     message_id = excluded.message_id,
                     action = excluded.action,
                     uid = NULL,
                     uid_validity = NULL,
                     status = 'pending',
                     attempts = 0,
                     next_attempt_at = excluded.next_attempt_at,
                     last_error = NULL,
                     last_error_kind = NULL,
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
                params![
                    Uuid::new_v4().to_string(),
                    message_id,
                    source_id,
                    external_id
                ],
            )?;
            Ok(())
        })
    }

    /// 邮件标已读即入队。UID 与 UIDVALIDITY 从入库时写下的 `payload_json` 里取，
    /// 不回表解析 MIME，也不重新连服务器；取不到就不入队（历史数据、非邮件来源）。
    pub fn enqueue_email_seen(&self, message_id: &str) -> DbResult<()> {
        self.database.with_connection(|connection| {
            connection.execute(
                "INSERT INTO inbox_read_back(
                     id, message_id, source_id, external_id, action, uid, uid_validity,
                     next_attempt_at
                 )
                 SELECT ?1, m.id, m.source_id, m.external_id, 'email_seen',
                        json_extract(m.payload_json, '$.uid'),
                        json_extract(m.payload_json, '$.uidValidity'),
                        strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 FROM inbox_messages m
                 WHERE m.id = ?2
                   AND json_type(m.payload_json, '$.uid') = 'integer'
                   AND json_type(m.payload_json, '$.uidValidity') = 'integer'
                 ON CONFLICT(source_id, external_id) DO UPDATE SET
                     message_id = excluded.message_id,
                     action = excluded.action,
                     uid = excluded.uid,
                     uid_validity = excluded.uid_validity,
                     status = 'pending',
                     attempts = 0,
                     next_attempt_at = excluded.next_attempt_at,
                     last_error = NULL,
                     last_error_kind = NULL,
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
                params![Uuid::new_v4().to_string(), message_id],
            )?;
            Ok(())
        })
    }

    /// 到期的一批：按退避时间升序，保证先失败的最先重试。
    pub fn due(&self, now: &str, limit: u32) -> DbResult<Vec<InboxReadBack>> {
        self.due_inner(now, None, limit)
    }

    /// 只取某一种动作的到期行。两种动作的批量上限不同：
    /// GitHub 一条一个请求，邮件一次连接可以带 200 个 UID。
    pub fn due_action(
        &self,
        now: &str,
        action: ReadBackAction,
        limit: u32,
    ) -> DbResult<Vec<InboxReadBack>> {
        self.due_inner(now, Some(action), limit)
    }

    fn due_inner(
        &self,
        now: &str,
        action: Option<ReadBackAction>,
        limit: u32,
    ) -> DbResult<Vec<InboxReadBack>> {
        let mut sql = format!(
            "SELECT {READ_BACK_FIELDS} FROM inbox_read_back
             WHERE status = 'pending' AND next_attempt_at <= ?1"
        );
        if action.is_some() {
            sql.push_str(" AND action = ?3");
        }
        sql.push_str(" ORDER BY next_attempt_at ASC, id ASC LIMIT ?2");

        self.database.with_connection(|connection| {
            let mut statement = connection.prepare(&sql)?;
            let rows = match action {
                Some(action) => {
                    statement.query_map(params![now, limit, action.as_str()], raw_read_back)?
                }
                None => statement.query_map(params![now, limit], raw_read_back)?,
            };
            rows.map(|row| decode_read_back(row?)).collect()
        })
    }

    pub fn get(&self, id: &str) -> DbResult<Option<InboxReadBack>> {
        self.database.with_connection(|connection| {
            let result = connection
                .query_row(
                    &format!("SELECT {READ_BACK_FIELDS} FROM inbox_read_back WHERE id = ?1"),
                    [id],
                    raw_read_back,
                )
                .optional()?;
            result.map(decode_read_back).transpose()
        })
    }

    /// 记录一次可恢复的失败，并安排下一次尝试时间。
    pub fn mark_retry(
        &self,
        id: &str,
        attempts: u32,
        next_attempt_at: &str,
        error: &str,
        kind: ReadBackErrorKind,
    ) -> DbResult<()> {
        self.database.with_connection(|connection| {
            connection.execute(
                "UPDATE inbox_read_back
                 SET status = 'pending',
                     attempts = ?2,
                     next_attempt_at = ?3,
                     last_error = ?4,
                     last_error_kind = ?5,
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1",
                params![id, attempts, next_attempt_at, error, kind.as_str()],
            )?;
            Ok(())
        })
    }

    /// 需要人工介入：认证失败或退避耗尽，退出自动重试。
    pub fn mark_blocked(
        &self,
        id: &str,
        attempts: u32,
        error: &str,
        kind: ReadBackErrorKind,
    ) -> DbResult<()> {
        self.database.with_connection(|connection| {
            connection.execute(
                "UPDATE inbox_read_back
                 SET status = 'blocked',
                     attempts = ?2,
                     last_error = ?3,
                     last_error_kind = ?4,
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1",
                params![id, attempts, error, kind.as_str()],
            )?;
            Ok(())
        })
    }

    pub fn remove(&self, id: &str) -> DbResult<usize> {
        self.database.with_connection(|connection| {
            connection
                .execute("DELETE FROM inbox_read_back WHERE id = ?1", [id])
                .map_err(Into::into)
        })
    }

    /// 本地改回未读时丢弃这条意图，否则远端会被标已读、本地却显示未读。
    pub fn remove_for_message(&self, message_id: &str) -> DbResult<usize> {
        self.database.with_connection(|connection| {
            connection
                .execute(
                    "DELETE FROM inbox_read_back WHERE message_id = ?1",
                    [message_id],
                )
                .map_err(Into::into)
        })
    }

    /// 同步可能把消息重新变成未读（远端有新动态），此时旧的「已读」意图作废。
    pub fn remove_unread(&self) -> DbResult<usize> {
        self.database.with_connection(|connection| {
            connection
                .execute(
                    "DELETE FROM inbox_read_back
                     WHERE message_id IN (SELECT id FROM inbox_messages WHERE read_at IS NULL)",
                    [],
                )
                .map_err(Into::into)
        })
    }

    /// 删除 Token 后 GitHub 的 pending 行无法写入，直接丢弃；blocked 保留，继续提示需要授权。
    /// 邮件行用各自的授权码，不受 GitHub Token 影响，这里必须按动作限定。
    pub fn clear_pending(&self) -> DbResult<usize> {
        self.database.with_connection(|connection| {
            connection
                .execute(
                    "DELETE FROM inbox_read_back
                     WHERE status = 'pending' AND action = ?1",
                    [ReadBackAction::GithubRead.as_str()],
                )
                .map_err(Into::into)
        })
    }

    /// 某一种动作是否有需要人工处理的行：判断「重试」按钮该先要求配哪种凭据。
    pub fn has_blocked(&self, action: ReadBackAction) -> DbResult<bool> {
        self.database.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT EXISTS(
                         SELECT 1 FROM inbox_read_back
                         WHERE status = 'blocked' AND action = ?1
                     )",
                    [action.as_str()],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(Into::into)
        })
    }

    /// 重新保存 Token 后让 blocked 行回到自动重试，不必等用户逐条点重试。
    pub fn reset_blocked(&self) -> DbResult<usize> {
        self.database.with_connection(|connection| {
            connection
                .execute(
                    "UPDATE inbox_read_back
                     SET status = 'pending',
                         attempts = 0,
                         next_attempt_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                         last_error = NULL,
                         last_error_kind = NULL,
                         updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                     WHERE status = 'blocked'",
                    [],
                )
                .map_err(Into::into)
        })
    }

    pub fn status(&self) -> DbResult<InboxReadBackStatus> {
        self.database.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT
                         COALESCE(SUM(CASE WHEN status = 'pending' THEN 1 ELSE 0 END), 0),
                         COALESCE(SUM(CASE WHEN status = 'blocked' THEN 1 ELSE 0 END), 0),
                         (SELECT last_error FROM inbox_read_back
                           WHERE status = 'blocked' AND last_error IS NOT NULL
                           ORDER BY updated_at DESC, id ASC
                           LIMIT 1)
                     FROM inbox_read_back",
                    [],
                    |row| {
                        Ok(InboxReadBackStatus {
                            pending: row.get(0)?,
                            blocked: row.get(1)?,
                            last_error: row.get(2)?,
                        })
                    },
                )
                .map_err(Into::into)
        })
    }
}

fn raw_read_back(row: &Row<'_>) -> rusqlite::Result<RawReadBack> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
        row.get(10)?,
        row.get(11)?,
    ))
}

fn decode_read_back(row: RawReadBack) -> DbResult<InboxReadBack> {
    Ok(InboxReadBack {
        id: row.0,
        message_id: row.1,
        source_id: row.2,
        external_id: row.3,
        action: parse_value(row.4, "inbox_read_back.action")?,
        uid: row.5,
        uid_validity: row.6,
        status: parse_value(row.7, "inbox_read_back.status")?,
        attempts: u32::try_from(row.8.max(0)).unwrap_or(u32::MAX),
        next_attempt_at: row.9,
        last_error: row.10,
        last_error_kind: row
            .11
            .map(|value| parse_value::<ReadBackErrorKind>(value, "inbox_read_back.last_error_kind"))
            .transpose()?,
    })
}
