use rusqlite::{OptionalExtension, Row, params};

use crate::{
    domain::{InboxSource, InboxSourceKind},
    infrastructure::db::{Database, DbError, DbResult},
};

use super::parse_value;

/// 开发用 ingest 需要一个稳定的来源；MVP 不暴露消息源管理界面，
/// 因此固定复用这一条记录，连接器接入后各自新建自己的来源。
pub const MANUAL_SOURCE_ID: &str = "source-manual";
const MANUAL_SOURCE_NAME: &str = "手动导入";

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
            connection.execute(
                "INSERT INTO inbox_sources(id, kind, name) VALUES (?1, ?2, ?3)
                 ON CONFLICT(id) DO NOTHING",
                params![
                    MANUAL_SOURCE_ID,
                    InboxSourceKind::Manual.as_str(),
                    MANUAL_SOURCE_NAME
                ],
            )?;
            connection
                .query_row(
                    &format!("SELECT {SOURCE_FIELDS} FROM inbox_sources WHERE id = ?1"),
                    [MANUAL_SOURCE_ID],
                    raw_source,
                )
                .optional()?
                .map(decode_source)
                .transpose()?
                .ok_or(DbError::NotFound("inbox source"))
        })
    }
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
