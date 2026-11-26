use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::{
    domain::{InboxSource, InboxSourceKind},
    infrastructure::db::{Database, DbError, DbResult},
};

use super::parse_value;

/// 开发用 ingest 需要一个稳定的来源；MVP 不暴露消息源管理界面，
/// 因此固定复用这一条记录，连接器接入后各自新建自己的来源。
pub const MANUAL_SOURCE_ID: &str = "source-manual";
const MANUAL_SOURCE_NAME: &str = "手动导入";

/// GitHub 通知连接器固定使用这一条来源，避免每次同步都新建来源。
pub const GITHUB_SOURCE_ID: &str = "source-github";
const GITHUB_SOURCE_NAME: &str = "GitHub 通知";

const SOURCE_FIELDS: &str =
    "id, kind, name, enabled, last_synced_at, last_error, created_at, updated_at";

type RawSource = (
    String,
    String,
    String,
    bool,
    Option<String>,
    Option<String>,
    String,
    String,
);

pub struct InboxSourceRepository {
    database: Database,
}

impl InboxSourceRepository {
    pub(crate) fn new(database: Database) -> Self {
        Self { database }
    }

    pub fn list(&self) -> DbResult<Vec<InboxSource>> {
        self.database.with_connection(|connection| {
            let mut statement = connection.prepare(&format!(
                "SELECT {SOURCE_FIELDS} FROM inbox_sources ORDER BY created_at ASC, id ASC"
            ))?;
            let rows = statement.query_map([], raw_source)?;
            rows.map(|row| decode_source(row?)).collect()
        })
    }

    pub fn get(&self, id: &str) -> DbResult<Option<InboxSource>> {
        self.database.with_connection(|connection| {
            let result = connection
                .query_row(
                    &format!("SELECT {SOURCE_FIELDS} FROM inbox_sources WHERE id = ?1"),
                    [id],
                    raw_source,
                )
                .optional()?;
            result.map(decode_source).transpose()
        })
    }

    /// 首次 ingest 时创建人工来源，之后直接复用；重复调用不会产生第二条。
    pub fn ensure_manual(&self) -> DbResult<InboxSource> {
        self.database.with_connection(|connection| {
            ensure_source(
                connection,
                MANUAL_SOURCE_ID,
                InboxSourceKind::Manual,
                MANUAL_SOURCE_NAME,
            )
        })
    }

    /// GitHub 连接器首次同步时创建来源，之后复用同一条。
    pub fn ensure_github(&self) -> DbResult<InboxSource> {
        self.database.with_connection(|connection| {
            ensure_source(
                connection,
                GITHUB_SOURCE_ID,
                InboxSourceKind::Github,
                GITHUB_SOURCE_NAME,
            )
        })
    }

    /// 同步游标不进 IPC，连接器单独读取。
    pub fn cursor(&self, id: &str) -> DbResult<Option<String>> {
        self.database.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT cursor FROM inbox_sources WHERE id = ?1",
                    [id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()
                .map(|value| value.flatten())
                .map_err(Into::into)
        })
    }

    /// 同步成功后更新游标与时间，并清掉上一次的错误。
    pub fn mark_synced(&self, id: &str, cursor: Option<&str>, synced_at: &str) -> DbResult<()> {
        self.database.with_connection(|connection| {
            connection.execute(
                "UPDATE inbox_sources
                 SET last_synced_at = ?2,
                     cursor = COALESCE(?3, cursor),
                     last_error = NULL,
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1",
                params![id, synced_at, cursor],
            )?;
            Ok(())
        })
    }

    /// 同步失败只记录错误，不动游标：下次仍从上次成功的位置继续。
    pub fn mark_failed(&self, id: &str, error: &str) -> DbResult<()> {
        self.database.with_connection(|connection| {
            connection.execute(
                "UPDATE inbox_sources
                 SET last_error = ?2,
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1",
                params![id, error],
            )?;
            Ok(())
        })
    }
}

fn ensure_source(
    connection: &Connection,
    id: &str,
    kind: InboxSourceKind,
    name: &str,
) -> DbResult<InboxSource> {
    connection.execute(
        "INSERT INTO inbox_sources(id, kind, name) VALUES (?1, ?2, ?3)
         ON CONFLICT(id) DO NOTHING",
        params![id, kind.as_str(), name],
    )?;
    connection
        .query_row(
            &format!("SELECT {SOURCE_FIELDS} FROM inbox_sources WHERE id = ?1"),
            [id],
            raw_source,
        )
        .optional()?
        .map(decode_source)
        .transpose()?
        .ok_or(DbError::NotFound("inbox source"))
}

fn raw_source(row: &Row<'_>) -> rusqlite::Result<RawSource> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
    ))
}

fn decode_source(row: RawSource) -> DbResult<InboxSource> {
    Ok(InboxSource {
        id: row.0,
        kind: parse_value(row.1, "inbox source kind")?,
        name: row.2,
        enabled: row.3,
        last_synced_at: row.4,
        last_error: row.5,
        created_at: row.6,
        updated_at: row.7,
    })
}
