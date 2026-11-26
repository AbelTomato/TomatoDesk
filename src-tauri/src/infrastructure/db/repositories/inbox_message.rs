use rusqlite::{Connection, OptionalExtension, Row, params, params_from_iter, types::Value};
use uuid::Uuid;

use crate::{
    domain::{
        InboxCounts, InboxMessage, InboxMessageCursor, InboxPage, InboxPageQuery, NewInboxMessage,
        NewTask, Task,
    },
    infrastructure::db::{Database, DbError, DbResult},
};

use super::{
    parse_value,
    task::{get_task, insert_task},
};

/// 收件箱消息按 `occurred_at` 倒序分页，索引与排序保持一致。
const MESSAGE_FIELDS: &str = "id, source_id, external_id, category, title, summary, body, url, \
                             actor, occurred_at, read_at, triaged_at, triage_outcome, \
                             triage_ref_id, related_kind, related_id";

type RawMessage = (
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

pub struct InboxMessageRepository {
    database: Database,
}

impl InboxMessageRepository {
    pub(crate) fn new(database: Database) -> Self {
        Self { database }
    }

    /// 按 `(source_id, external_id)` 去重写入。
    ///
    /// 规则（见方案 5.6）：
    /// - 已判断的消息不因重新同步而恢复未读，判断结果也不会被覆盖；
    /// - 未判断的消息如果有新进展，重新变为未读；
    /// - `related_*` 只做补写，不会被空值清掉。
    pub fn ingest(&self, message: NewInboxMessage) -> DbResult<InboxMessage> {
        if message.title.trim().is_empty() {
            return Err(DbError::InvalidData(
                "inbox message title cannot be empty".into(),
            ));
        }
        if message.category.trim().is_empty() {
            return Err(DbError::InvalidData(
                "inbox message category cannot be empty".into(),
            ));
        }

        let id = Uuid::new_v4().to_string();
        self.database.with_connection(|connection| {
            connection.execute(
                "INSERT INTO inbox_messages(
                     id, source_id, external_id, external_updated_at, category, title, summary,
                     body, url, actor, occurred_at, related_kind, related_id, payload_json
                 )
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                         COALESCE(?11, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                         ?12, ?13, ?14)
                 ON CONFLICT(source_id, external_id) DO UPDATE SET
                     title = excluded.title,
                     summary = excluded.summary,
                     body = excluded.body,
                     url = excluded.url,
                     actor = excluded.actor,
                     occurred_at = excluded.occurred_at,
                     external_updated_at = excluded.external_updated_at,
                     related_kind = COALESCE(excluded.related_kind, inbox_messages.related_kind),
                     related_id = COALESCE(excluded.related_id, inbox_messages.related_id),
                     payload_json = COALESCE(excluded.payload_json, inbox_messages.payload_json),
                     read_at = CASE
                         WHEN inbox_messages.triaged_at IS NOT NULL
                             THEN inbox_messages.read_at
                         WHEN excluded.external_updated_at > COALESCE(inbox_messages.external_updated_at, '')
                             THEN NULL
                         ELSE inbox_messages.read_at
                     END",
                params![
                    id,
                    message.source_id,
                    message.external_id,
                    message.external_updated_at,
                    message.category,
                    message.title,
                    message.summary,
                    message.body,
                    message.url,
                    message.actor,
                    message.occurred_at,
                    message.related_kind,
                    message.related_id,
                    message.payload_json
                ],
            )?;
            connection
                .query_row(
                    &format!(
                        "SELECT {MESSAGE_FIELDS} FROM inbox_messages
                         WHERE source_id = ?1 AND external_id = ?2"
                    ),
                    params![message.source_id, message.external_id],
                    raw_message,
                )
                .optional()?
                .map(decode_message)
                .transpose()?
                .ok_or(DbError::NotFound("inbox message"))
        })
    }

    pub fn get(&self, id: &str) -> DbResult<Option<InboxMessage>> {
        self.database
            .with_connection(|connection| get_message(connection, id))
    }

    /// 键集分页：多取一行判断是否还有下一页，筛选全部下沉到 SQL。
    pub fn page(&self, query: &InboxPageQuery) -> DbResult<InboxPage> {
        let limit = query.limit.clamp(1, 200);
        let mut sql = format!("SELECT {MESSAGE_FIELDS} FROM inbox_messages WHERE 1 = 1");
        let mut values: Vec<Value> = Vec::new();

        match query.triaged {
            Some(true) => sql.push_str(" AND triaged_at IS NOT NULL"),
            Some(false) => sql.push_str(" AND triaged_at IS NULL"),
            None => {}
        }
        if query.unread_only {
            sql.push_str(" AND read_at IS NULL");
        }
        if let Some(category) = &query.category {
            sql.push_str(" AND category = ?");
            values.push(Value::Text(category.clone()));
        }
        if let Some(source_id) = &query.source_id {
            sql.push_str(" AND source_id = ?");
            values.push(Value::Text(source_id.clone()));
        }
        if let Some(cursor) = &query.cursor {
            sql.push_str(" AND (occurred_at < ? OR (occurred_at = ? AND id < ?))");
            values.push(Value::Text(cursor.value.clone()));
            values.push(Value::Text(cursor.value.clone()));
            values.push(Value::Text(cursor.id.clone()));
        }
        sql.push_str(" ORDER BY occurred_at DESC, id DESC LIMIT ?");
        values.push(Value::Integer(i64::from(limit) + 1));

        self.database.with_connection(|connection| {
            let mut statement = connection.prepare(&sql)?;
            let rows = statement.query_map(params_from_iter(values.iter()), raw_message)?;
            let mut messages = rows
                .map(|row| decode_message(row?))
                .collect::<DbResult<Vec<_>>>()?;

            let has_more = messages.len() > limit as usize;
            if has_more {
                messages.truncate(limit as usize);
            }
            let next_cursor = if has_more {
                messages.last().map(|message| {
                    InboxMessageCursor {
                        value: message.occurred_at.clone(),
                        id: message.id.clone(),
                    }
                    .encode()
                })
            } else {
                None
            };

            Ok(InboxPage {
                messages,
                next_cursor,
                has_more,
            })
        })
    }

    /// 三个计数一次查完；导航角标只取 `pending`。
    pub fn counts(&self) -> DbResult<InboxCounts> {
        self.database.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT COUNT(*),
                            COALESCE(SUM(CASE WHEN triaged_at IS NULL THEN 1 ELSE 0 END), 0),
                            COALESCE(SUM(CASE WHEN read_at IS NULL AND triaged_at IS NULL
                                              THEN 1 ELSE 0 END), 0)
                     FROM inbox_messages",
                    [],
                    |row| {
                        Ok(InboxCounts {
                            all: row.get(0)?,
                            pending: row.get(1)?,
                            unread: row.get(2)?,
                        })
                    },
                )
                .map_err(Into::into)
        })
    }

    /// 已读 / 未读只改 `read_at`；已判断的消息也可以标记，方便回看。
    pub fn set_read(&self, id: &str, read: bool) -> DbResult<Option<InboxMessage>> {
        self.database.with_connection(|connection| {
            let changed = connection.execute(
                "UPDATE inbox_messages
                 SET read_at = CASE
                         WHEN ?2 = 1 THEN COALESCE(read_at, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                         ELSE NULL
                     END
                 WHERE id = ?1",
                params![id, i64::from(read)],
            )?;
            if changed == 0 {
                return Ok(None);
            }
            get_message(connection, id)
        })
    }

    /// 忽略是软删除：只写判断结果，消息保留用于去重与审计。
    /// 已经判断过的消息不会被改写，直接返回原状，重复点击是幂等的。
    pub fn dismiss(&self, id: &str) -> DbResult<Option<InboxMessage>> {
        self.database.with_connection(|connection| {
            connection.execute(
                "UPDATE inbox_messages
                 SET triaged_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                     triage_outcome = 'dismissed',
                     triage_ref_id = NULL
                 WHERE id = ?1 AND triaged_at IS NULL",
                [id],
            )?;
            get_message(connection, id)
        })
    }

    /// 转为待办：同一事务内写待办并回填消息的判断结果。
    ///
    /// `WHERE triaged_at IS NULL` 是防止重复判断的第一道防线，影响 0 行就回滚整个事务；
    /// `tasks_inbox_origin_unique` 是第二道防线。
    pub fn to_task(&self, id: &str, task: NewTask) -> DbResult<Task> {
        let task_id = Uuid::new_v4().to_string();
        self.database.with_transaction(|transaction| {
            let changed = transaction.execute(
                "UPDATE inbox_messages
                 SET triaged_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                     triage_outcome = 'task',
                     triage_ref_id = ?2
                 WHERE id = ?1 AND triaged_at IS NULL",
                params![id, task_id],
            )?;
            if changed == 0 {
                return Err(DbError::InvalidData(
                    "inbox message is already triaged".into(),
                ));
            }
            insert_task(transaction, &task_id, task)?;
            get_task(transaction, &task_id)?.ok_or(DbError::NotFound("task"))
        })
    }
}

fn get_message(connection: &Connection, id: &str) -> DbResult<Option<InboxMessage>> {
    let result = connection
        .query_row(
            &format!("SELECT {MESSAGE_FIELDS} FROM inbox_messages WHERE id = ?1"),
            [id],
            raw_message,
        )
        .optional()?;
    result.map(decode_message).transpose()
}

fn raw_message(row: &Row<'_>) -> rusqlite::Result<RawMessage> {
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
        row.get(12)?,
        row.get(13)?,
        row.get(14)?,
        row.get(15)?,
    ))
}

fn decode_message(row: RawMessage) -> DbResult<InboxMessage> {
    Ok(InboxMessage {
        id: row.0,
        source_id: row.1,
        external_id: row.2,
        category: row.3,
        title: row.4,
        summary: row.5,
        body: row.6,
        url: row.7,
        actor: row.8,
        occurred_at: row.9,
        read_at: row.10,
        triaged_at: row.11,
        triage_outcome: row
            .12
            .map(|value| parse_value(value, "triage outcome"))
            .transpose()?,
        triage_ref_id: row.13,
        related_kind: row.14,
        related_id: row.15,
    })
}
