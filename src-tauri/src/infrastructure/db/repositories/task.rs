use rusqlite::{Connection, OptionalExtension, Row, params, params_from_iter, types::Value};
use uuid::Uuid;

use crate::{
    domain::{NewTask, Task, TaskCounts, TaskCursor, TaskPage, TaskPageQuery, TaskStatus},
    infrastructure::db::{Database, DbError, DbResult},
};

use super::parse_value;

type RawTask = (
    String,
    String,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    Option<String>,
);

const TASK_FIELDS: &str =
    "id, title, note, status, origin_kind, origin_id, created_at, updated_at, completed_at";

pub struct TaskRepository {
    database: Database,
}

impl TaskRepository {
    pub(crate) fn new(database: Database) -> Self {
        Self { database }
    }

    pub fn create(&self, task: NewTask) -> DbResult<Task> {
        if task.title.trim().is_empty() {
            return Err(DbError::InvalidData("task title cannot be empty".into()));
        }
        let id = Uuid::new_v4().to_string();
        self.database.with_connection(|connection| {
            insert_task(connection, &id, task)?;
            get_task(connection, &id)?.ok_or(DbError::NotFound("task"))
        })
    }

    pub fn get(&self, id: &str) -> DbResult<Option<Task>> {
        self.database
            .with_connection(|connection| get_task(connection, id))
    }

    /// 键集分页：多取一行判断是否还有下一页，排序与索引一致（updated_at DESC, id DESC）。
    pub fn page(&self, query: &TaskPageQuery) -> DbResult<TaskPage> {
        let limit = query.limit.clamp(1, 200);
        let mut sql = format!("SELECT {TASK_FIELDS} FROM tasks WHERE 1 = 1");
        let mut values: Vec<Value> = Vec::new();

        if let Some(status) = query.status {
            sql.push_str(" AND status = ?");
            values.push(Value::Text(status.as_str().to_owned()));
        }
        if let Some(cursor) = &query.cursor {
            sql.push_str(" AND (updated_at < ? OR (updated_at = ? AND id < ?))");
            values.push(Value::Text(cursor.value.clone()));
            values.push(Value::Text(cursor.value.clone()));
            values.push(Value::Text(cursor.id.clone()));
        }
        sql.push_str(" ORDER BY updated_at DESC, id DESC LIMIT ?");
        values.push(Value::Integer(i64::from(limit) + 1));

        self.database.with_connection(|connection| {
            let mut statement = connection.prepare(&sql)?;
            let rows = statement.query_map(params_from_iter(values.iter()), raw_task)?;
            let mut tasks = rows
                .map(|row| decode_task(row?))
                .collect::<DbResult<Vec<_>>>()?;

            let has_more = tasks.len() > limit as usize;
            if has_more {
                tasks.truncate(limit as usize);
            }
            let next_cursor = if has_more {
                tasks.last().map(|task| {
                    TaskCursor {
                        value: task.updated_at.clone(),
                        id: task.id.clone(),
                    }
                    .encode()
                })
            } else {
                None
            };

            Ok(TaskPage {
                tasks,
                next_cursor,
                has_more,
            })
        })
    }

    /// 四个计数一次查完，导航角标只取 `open`。
    pub fn counts(&self) -> DbResult<TaskCounts> {
        self.database.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT COUNT(*),
                            COALESCE(SUM(CASE WHEN status = 'open' THEN 1 ELSE 0 END), 0),
                            COALESCE(SUM(CASE WHEN status = 'done' THEN 1 ELSE 0 END), 0),
                            COALESCE(SUM(CASE WHEN status = 'cancelled' THEN 1 ELSE 0 END), 0)
                     FROM tasks",
                    [],
                    |row| {
                        Ok(TaskCounts {
                            all: row.get(0)?,
                            open: row.get(1)?,
                            done: row.get(2)?,
                            cancelled: row.get(3)?,
                        })
                    },
                )
                .map_err(Into::into)
        })
    }

    /// 只改标题与备注；状态走 `set_status`，来源创建后不再变化。
    pub fn update(&self, id: &str, title: &str, note: Option<&str>) -> DbResult<Option<Task>> {
        if title.trim().is_empty() {
            return Err(DbError::InvalidData("task title cannot be empty".into()));
        }
        self.database.with_connection(|connection| {
            connection.execute(
                "UPDATE tasks
                 SET title = ?2, note = ?3, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1",
                params![id, title, note],
            )?;
            get_task(connection, id)
        })
    }

    /// 完成 / 取消 / 重开。重复完成保留第一次的完成时间，取消与重开清空它。
    pub fn set_status(&self, id: &str, status: TaskStatus) -> DbResult<Option<Task>> {
        self.database.with_connection(|connection| {
            let changed = connection.execute(
                "UPDATE tasks
                 SET status = ?2,
                     completed_at = CASE
                         WHEN ?2 = 'done' THEN COALESCE(completed_at, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                         ELSE NULL
                     END,
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1",
                params![id, status.as_str()],
            )?;
            if changed == 0 {
                return Ok(None);
            }
            get_task(connection, id)
        })
    }
}

/// 写入一条待办，供本仓储与收件箱的「转为待办」事务共用。
/// 调用方负责生成 id，这样才能在同一个事务里先写待办、再回填消息的 `triage_ref_id`。
pub(super) fn insert_task(connection: &Connection, id: &str, task: NewTask) -> DbResult<()> {
    if task.title.trim().is_empty() {
        return Err(DbError::InvalidData("task title cannot be empty".into()));
    }
    connection.execute(
        "INSERT INTO tasks(id, title, note, origin_kind, origin_id)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, task.title, task.note, task.origin_kind, task.origin_id],
    )?;
    Ok(())
}

pub(super) fn get_task(connection: &Connection, id: &str) -> DbResult<Option<Task>> {
    let result = connection
        .query_row(
            &format!("SELECT {TASK_FIELDS} FROM tasks WHERE id = ?1"),
            [id],
            raw_task,
        )
        .optional()?;
    result.map(decode_task).transpose()
}

fn raw_task(row: &Row<'_>) -> rusqlite::Result<RawTask> {
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

fn decode_task(row: RawTask) -> DbResult<Task> {
    Ok(Task {
        id: row.0,
        title: row.1,
        note: row.2,
        status: parse_value(row.3, "task status")?,
        origin_kind: row.4,
        origin_id: row.5,
        created_at: row.6,
        updated_at: row.7,
        completed_at: row.8,
    })
}
