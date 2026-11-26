use rusqlite::{OptionalExtension, Row, params};
use uuid::Uuid;

use crate::{
    domain::{InboxReadBack, InboxReadBackStatus, ReadBackErrorKind},
    infrastructure::db::{Database, DbResult},
};

use super::parse_value;

const READ_BACK_FIELDS: &str = "id, message_id, source_id, external_id, status, attempts, \
                                next_attempt_at, last_error, last_error_kind";

type RawReadBack = (
    String,
    String,
    String,
    String,
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

    /// 标已读即入队：重复标已读会把退避归零，
    /// 用户的主动动作优先于上一次失败留下的等待时间。
    pub fn enqueue(&self, message_id: &str, source_id: &str, external_id: &str) -> DbResult<()> {
        self.database.with_connection(|connection| {
            connection.execute(
                "INSERT INTO inbox_read_back(id, message_id, source_id, external_id, next_attempt_at)
                 VALUES (?1, ?2, ?3, ?4, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                 ON CONFLICT(source_id, external_id) DO UPDATE SET
                     message_id = excluded.message_id,
                     status = 'pending',
                     attempts = 0,
                     next_attempt_at = excluded.next_attempt_at,
                     last_error = NULL,
                     last_error_kind = NULL,
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
                params![Uuid::new_v4().to_string(), message_id, source_id, external_id],
            )?;
            Ok(())
        })
    }

    /// 到期的一批：按退避时间升序，保证先失败的最先重试。
    pub fn due(&self, now: &str, limit: u32) -> DbResult<Vec<InboxReadBack>> {
        self.database.with_connection(|connection| {
            let mut statement = connection.prepare(&format!(
                "SELECT {READ_BACK_FIELDS} FROM inbox_read_back
                 WHERE status = 'pending' AND next_attempt_at <= ?1
                 ORDER BY next_attempt_at ASC, id ASC
                 LIMIT ?2"
            ))?;
            let rows = statement.query_map(params![now, limit], raw_read_back)?;
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

    /// 删除 Token 后 pending 行无法写入，直接丢弃；blocked 保留，继续提示需要授权。
    pub fn clear_pending(&self) -> DbResult<usize> {
        self.database.with_connection(|connection| {
            connection
                .execute("DELETE FROM inbox_read_back WHERE status = 'pending'", [])
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
    ))
}

fn decode_read_back(row: RawReadBack) -> DbResult<InboxReadBack> {
    Ok(InboxReadBack {
        id: row.0,
        message_id: row.1,
        source_id: row.2,
        external_id: row.3,
        status: parse_value(row.4, "inbox_read_back.status")?,
        attempts: u32::try_from(row.5.max(0)).unwrap_or(u32::MAX),
        next_attempt_at: row.6,
        last_error: row.7,
        last_error_kind: row
            .8
            .map(|value| parse_value::<ReadBackErrorKind>(value, "inbox_read_back.last_error_kind"))
            .transpose()?,
    })
}
