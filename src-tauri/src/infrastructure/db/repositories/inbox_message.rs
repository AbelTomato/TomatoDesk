use rusqlite::{Connection, OptionalExtension, Row, params, params_from_iter, types::Value};
use uuid::Uuid;

use crate::{
    domain::{
        InboxAction, InboxActionResult, InboxCounts, InboxMessage, InboxMessageCursor, InboxPage,
        InboxPageQuery, InboxScope, InboxTab, IngestOutcome, NewInboxMessage, NewTask,
    },
    infrastructure::db::{Database, DbError, DbResult},
};

use super::{parse_value, task::insert_task};

/// 收件箱消息按 `occurred_at` 倒序分页，索引与排序保持一致。
const MESSAGE_FIELDS: &str = "id, source_id, external_id, category, title, summary, body, url, \
                             actor, occurred_at, read_at, triaged_at, triage_outcome, \
                             triage_ref_id, related_kind, related_id, triage_by";

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
    Option<String>,
);

pub struct InboxMessageRepository {
    database: Database,
}

impl InboxMessageRepository {
    pub(crate) fn new(database: Database) -> Self {
        Self { database }
    }

    /// 按 `(source_id, external_id)` 去重写入，并告知这条消息是不是新建的。
    ///
    /// 规则（见方案 5.6）：
    /// - 已判断的消息不因重新同步而恢复未读，判断结果也不会被覆盖；
    /// - 未判断的消息如果有新进展，重新变为未读；
    /// - `related_*` 只做补写，不会被空值清掉。
    ///
    /// `created` 是入库前先查一次得到的：规则只对新消息生效，重新同步的老消息
    /// 已经被人处理过，不该被规则再改一遍。
    pub fn ingest(&self, message: NewInboxMessage) -> DbResult<IngestOutcome> {
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
            let created = !message_exists(connection, &message.source_id, &message.external_id)?;
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
                .map(|message| IngestOutcome { message, created })
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

        push_scope(&mut sql, &mut values, &query.scope);
        if let Some(cursor) = &query.cursor {
            sql.push_str(" AND (occurred_at < ? OR (occurred_at = ? AND id < ?))");
            values.push(Value::Text(cursor.value.clone()));
            values.push(Value::Text(cursor.value.clone()));
            values.push(Value::Text(cursor.id.clone()));
        }
        sql.push_str(" ORDER BY occurred_at DESC, id DESC LIMIT ?");
        values.push(Value::Integer(i64::from(limit) + 1));

        self.database.with_connection(|connection| {
            let total = count_scope_in(connection, &query.scope)?;
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
                total,
            })
        })
    }

    /// 五个计数一次查完；导航角标只取 `pending`。
    ///
    /// 只按来源与分类过滤，不按 Tab 与未读过滤：四个 Tab 的数字必须能相加等于「全部」。
    pub fn counts(&self, source_id: Option<&str>, category: Option<&str>) -> DbResult<InboxCounts> {
        let mut sql = String::from(
            "SELECT COUNT(*),
                    COALESCE(SUM(CASE WHEN triaged_at IS NULL THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE WHEN triage_outcome = 'task' THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE WHEN triage_outcome = 'dismissed' THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE WHEN read_at IS NULL THEN 1 ELSE 0 END), 0)
             FROM inbox_messages WHERE 1 = 1",
        );
        let mut values: Vec<Value> = Vec::new();
        push_source_filters(&mut sql, &mut values, source_id, category);

        self.database.with_connection(|connection| {
            connection
                .query_row(&sql, params_from_iter(values.iter()), |row| {
                    Ok(InboxCounts {
                        all: row.get(0)?,
                        pending: row.get(1)?,
                        task: row.get(2)?,
                        dismissed: row.get(3)?,
                        unread: row.get(4)?,
                    })
                })
                .map_err(Into::into)
        })
    }

    /// 当前筛选下有多少条：`scope` 模式执行前用它和前端确认过的数字比对。
    pub fn count_scope(&self, scope: &InboxScope) -> DbResult<i64> {
        self.database
            .with_connection(|connection| count_scope_in(connection, scope))
    }

    /// 按 id 批量执行一个动作。整批一个事务：要么全成，要么一条都不改。
    pub fn apply_action(&self, action: InboxAction, ids: &[String]) -> DbResult<InboxActionResult> {
        self.database
            .with_transaction(|transaction| apply_in(transaction, action, ids))
    }

    /// 「选择全部 N 条」：先数一遍核对，再取 id 执行。两次查询在同一事务内，
    /// 避免中间插入的新消息让确认的数字与执行的范围错位。
    pub fn apply_action_scope(
        &self,
        action: InboxAction,
        scope: &InboxScope,
        expected: i64,
    ) -> DbResult<InboxActionResult> {
        self.database.with_transaction(|transaction| {
            let actual = count_scope_in(transaction, scope)?;
            if actual != expected {
                return Ok(InboxActionResult::stale(action, actual));
            }
            let ids = resolve_scope_ids_in(transaction, scope)?;
            apply_in(transaction, action, &ids)
        })
    }

    /// 撤销：把动作反向执行一遍。`action` 是当初执行的动作，不是要执行的反向动作。
    pub fn undo_action(
        &self,
        action: InboxAction,
        ids: &[String],
        task_ids: &[String],
    ) -> DbResult<InboxActionResult> {
        self.database
            .with_transaction(|transaction| undo_in(transaction, action, ids, task_ids))
    }

    /// 规则命中后的执行：动作与人工触发的完全一致，另外累加规则的命中统计。
    /// 两个写入在同一个事务里，不会出现「改了消息却没记命中」。
    ///
    /// `triage_by` 只标在真正的处理结果上：规则标已读不改变处理状态，
    /// 消息仍然是「待处理」，写这个字段会让详情与「规则」标记说反话。
    ///
    /// 只对真正变了的行计数：规则重复命中一条已经处理过的消息不算一次命中。
    pub fn apply_rule(
        &self,
        rule_id: &str,
        action: InboxAction,
        id: &str,
    ) -> DbResult<InboxActionResult> {
        self.database.with_transaction(|transaction| {
            let result = apply_in(transaction, action, &[id.to_owned()])?;
            if result.applied_ids.is_empty() {
                return Ok(result);
            }
            if action == InboxAction::Dismiss {
                transaction.execute(
                    "UPDATE inbox_messages SET triage_by = 'rule' WHERE id = ?1",
                    [id],
                )?;
            }
            transaction.execute(
                "UPDATE inbox_rules
                 SET hit_count = hit_count + 1,
                     last_hit_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1",
                [rule_id],
            )?;
            Ok(result)
        })
    }

    /// 已读 / 未读只改 `read_at`；已判断的消息也可以标记，方便回看。
    pub fn set_read(&self, id: &str, read: bool) -> DbResult<Option<InboxMessage>> {
        let action = if read {
            InboxAction::Read
        } else {
            InboxAction::Unread
        };
        self.apply_action(action, &[id.to_owned()])?;
        self.get(id)
    }

    /// 忽略是软删除：只写判断结果，消息保留用于去重与审计。
    /// 已经判断过的消息不会被改写，重复点击是幂等的。
    pub fn dismiss(&self, id: &str) -> DbResult<Option<InboxMessage>> {
        self.apply_action(InboxAction::Dismiss, &[id.to_owned()])?;
        self.get(id)
    }
}

/// 状态轴的四条判断对应四段 SQL；列表与「选择全部」共用，
/// 避免两处筛选条件各写一遍后慢慢漂移。
fn push_scope(sql: &mut String, values: &mut Vec<Value>, scope: &InboxScope) {
    match scope.tab {
        InboxTab::Pending => sql.push_str(" AND triaged_at IS NULL"),
        InboxTab::Task => sql.push_str(" AND triage_outcome = 'task'"),
        InboxTab::Dismissed => sql.push_str(" AND triage_outcome = 'dismissed'"),
        InboxTab::All => {}
    }
    if scope.unread_only {
        sql.push_str(" AND read_at IS NULL");
    }
    push_source_filters(
        sql,
        values,
        scope.source_id.as_deref(),
        scope.category.as_deref(),
    );
}

/// 来源与分类是「计数也跟随」的两个筛选，单独抽出来给 `counts` 复用。
fn push_source_filters(
    sql: &mut String,
    values: &mut Vec<Value>,
    source_id: Option<&str>,
    category: Option<&str>,
) {
    if let Some(category) = category {
        sql.push_str(" AND category = ?");
        values.push(Value::Text(category.to_owned()));
    }
    if let Some(source_id) = source_id {
        sql.push_str(" AND source_id = ?");
        values.push(Value::Text(source_id.to_owned()));
    }
}

/// 「选择全部」一次最多处理的条数：上万条会卡住主线程，也会放大误伤的代价。
/// 真实积压远小于这个数，够用且可控。
const MAX_SCOPE_ACTION: i64 = 2000;

/// 执行一个动作的内层实现。调用方保证已经在事务里。
///
/// 只统计真正变了行（`changed > 0`）的消息：撤销才不会把用户早就标过已读的
/// 消息翻回未读。已经处理过或已被别的路径改掉的行只记 `skipped`，不报错。
fn apply_in(
    connection: &Connection,
    action: InboxAction,
    ids: &[String],
) -> DbResult<InboxActionResult> {
    let mut result = InboxActionResult::new(action);
    for id in ids {
        match action {
            InboxAction::Read => {
                let changed = connection.execute(
                    "UPDATE inbox_messages
                     SET read_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                     WHERE id = ?1 AND read_at IS NULL",
                    [id],
                )?;
                record(&mut result, id, changed > 0);
            }
            InboxAction::Unread => {
                let changed = connection.execute(
                    "UPDATE inbox_messages SET read_at = NULL
                     WHERE id = ?1 AND read_at IS NOT NULL",
                    [id],
                )?;
                record(&mut result, id, changed > 0);
            }
            InboxAction::Dismiss => {
                let changed = connection.execute(
                    "UPDATE inbox_messages
                     SET triaged_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                         triage_outcome = 'dismissed',
                         triage_ref_id = NULL,
                         triage_by = NULL
                     WHERE id = ?1 AND triaged_at IS NULL",
                    [id],
                )?;
                record(&mut result, id, changed > 0);
            }
            InboxAction::Task => {
                // 待办标题取消息标题、备注取摘要与链接，所以先读一次消息。
                let Some(message) = get_message(connection, id)? else {
                    result.skipped += 1;
                    continue;
                };
                let task_id = Uuid::new_v4().to_string();
                let changed = connection.execute(
                    "UPDATE inbox_messages
                     SET triaged_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                         triage_outcome = 'task',
                         triage_ref_id = ?2,
                         triage_by = NULL
                     WHERE id = ?1 AND triaged_at IS NULL",
                    params![id, task_id],
                )?;
                if changed == 0 {
                    result.skipped += 1;
                    continue;
                }
                insert_task(
                    connection,
                    &task_id,
                    NewTask {
                        title: message.title.clone(),
                        note: message.task_note(),
                        origin_kind: Some("inbox".into()),
                        origin_id: Some(message.id),
                    },
                )?;
                result.applied_ids.push(id.clone());
                result.task_ids.push(task_id);
            }
        }
    }
    Ok(result)
}

/// 撤销的内层实现：`action` 是当初执行的动作，这里执行它的反向。
fn undo_in(
    connection: &Connection,
    action: InboxAction,
    ids: &[String],
    task_ids: &[String],
) -> DbResult<InboxActionResult> {
    let mut result = InboxActionResult::new(action);
    for id in ids {
        let changed = match action {
            InboxAction::Read => connection.execute(
                "UPDATE inbox_messages SET read_at = NULL
                 WHERE id = ?1 AND read_at IS NOT NULL",
                [id],
            )?,
            InboxAction::Unread => connection.execute(
                "UPDATE inbox_messages
                 SET read_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1 AND read_at IS NULL",
                [id],
            )?,
            // 清三个字段才算回到待处理；`triage_outcome` 的条件保证只撤销同一种结果。
            InboxAction::Dismiss => connection.execute(
                "UPDATE inbox_messages
                 SET triaged_at = NULL, triage_outcome = NULL, triage_ref_id = NULL,
                     triage_by = NULL
                 WHERE id = ?1 AND triage_outcome = 'dismissed'",
                [id],
            )?,
            InboxAction::Task => connection.execute(
                "UPDATE inbox_messages
                 SET triaged_at = NULL, triage_outcome = NULL, triage_ref_id = NULL,
                     triage_by = NULL
                 WHERE id = ?1 AND triage_outcome = 'task'",
                [id],
            )?,
        };
        record(&mut result, id, changed > 0);
    }

    // 待办保留，只解除与消息的绑定：标题、备注、完成状态都不动，
    // `origin_id` 置空后局部唯一索引不再拦路，用户可以再次转换。
    if action == InboxAction::Task {
        for task_id in task_ids {
            connection.execute(
                "UPDATE tasks
                 SET origin_id = NULL, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1 AND origin_kind = 'inbox'",
                [task_id],
            )?;
        }
    }
    Ok(result)
}

fn record(result: &mut InboxActionResult, id: &str, changed: bool) {
    if changed {
        result.applied_ids.push(id.to_owned());
    } else {
        result.skipped += 1;
    }
}

fn count_scope_in(connection: &Connection, scope: &InboxScope) -> DbResult<i64> {
    let mut sql = String::from("SELECT COUNT(*) FROM inbox_messages WHERE 1 = 1");
    let mut values: Vec<Value> = Vec::new();
    push_scope(&mut sql, &mut values, scope);
    connection
        .query_row(&sql, params_from_iter(values.iter()), |row| row.get(0))
        .map_err(Into::into)
}

/// 与列表同样的排序与筛选，「选择全部」处理的就是用户看到的那一批。
fn resolve_scope_ids_in(connection: &Connection, scope: &InboxScope) -> DbResult<Vec<String>> {
    let mut sql = String::from("SELECT id FROM inbox_messages WHERE 1 = 1");
    let mut values: Vec<Value> = Vec::new();
    push_scope(&mut sql, &mut values, scope);
    sql.push_str(" ORDER BY occurred_at DESC, id DESC LIMIT ?");
    values.push(Value::Integer(MAX_SCOPE_ACTION));

    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(values.iter()), |row| row.get(0))?;
    rows.collect::<rusqlite::Result<Vec<String>>>()
        .map_err(Into::into)
}

/// 入库前判存在：`ON CONFLICT` 不会告诉我们这次是插入还是更新。
fn message_exists(connection: &Connection, source_id: &str, external_id: &str) -> DbResult<bool> {
    connection
        .query_row(
            "SELECT 1 FROM inbox_messages WHERE source_id = ?1 AND external_id = ?2",
            params![source_id, external_id],
            |_| Ok(()),
        )
        .optional()
        .map(|row| row.is_some())
        .map_err(Into::into)
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
        row.get(16)?,
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
        triage_by: row.16,
        related_kind: row.14,
        related_id: row.15,
    })
}
