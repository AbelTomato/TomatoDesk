use rusqlite::{Connection, OptionalExtension, Row, params, params_from_iter, types::Value};
use uuid::Uuid;

use crate::{
    domain::{
        Capture, CaptureBody, CaptureCounts, CaptureCursor, CaptureKind, CapturePage,
        CapturePageQuery, CaptureSort, CaptureSummary, NewCapture,
    },
    infrastructure::db::{Database, DbError, DbResult},
};

use super::parse_value;

type RawCapture = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
    String,
);

type RawSummary = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
    String,
);

const CAPTURE_FIELDS: &str = "id, kind, source, url, title, summary, note, body, repo_id, archived_at, created_at, updated_at";
const SUMMARY_FIELDS: &str =
    "id, kind, source, url, title, summary, note, repo_id, archived_at, created_at, updated_at";
const SEARCH_SUMMARY_FIELDS: &str = "captures.id, captures.kind, captures.source, captures.url,
                                     captures.title, captures.summary, captures.note,
                                     captures.repo_id, captures.archived_at, captures.created_at,
                                     captures.updated_at";
const SEARCH_LIMIT: u32 = 100;

pub struct CaptureRepository {
    database: Database,
}

impl CaptureRepository {
    pub(crate) fn new(database: Database) -> Self {
        Self { database }
    }

    pub fn create(&self, capture: NewCapture) -> DbResult<Capture> {
        if capture.source.trim().is_empty() {
            return Err(DbError::InvalidData(
                "capture source cannot be empty".into(),
            ));
        }
        let id = Uuid::new_v4().to_string();
        self.database.with_connection(|connection| {
            connection.execute(
                "INSERT INTO captures(id, kind, source, url, title, summary, note, body, repo_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    id,
                    capture.kind.as_str(),
                    capture.source,
                    capture.url,
                    capture.title,
                    capture.summary,
                    capture.note,
                    capture.body,
                    capture.repo_id
                ],
            )?;
            get_capture(connection, &id)?.ok_or(DbError::NotFound("capture"))
        })
    }

    pub fn get(&self, id: &str) -> DbResult<Option<Capture>> {
        self.database
            .with_connection(|connection| get_capture(connection, id))
    }

    pub fn list(&self) -> DbResult<Vec<Capture>> {
        self.database.with_connection(|connection| {
            let mut statement = connection.prepare(&format!(
                "SELECT {CAPTURE_FIELDS} FROM captures ORDER BY updated_at DESC, id DESC"
            ))?;
            let rows = statement.query_map([], raw_capture)?;
            rows.map(|row| decode_capture(row?)).collect()
        })
    }

    /// 键集分页：一次多取一行用于判断是否还有下一页，避免额外的 count 查询。
    pub fn page(&self, query: &CapturePageQuery) -> DbResult<CapturePage> {
        let limit = query.limit.clamp(1, 200);
        let column = query.sort.column();
        let mut sql = format!("SELECT {SUMMARY_FIELDS} FROM captures WHERE 1 = 1");
        let mut values: Vec<Value> = Vec::new();

        match query.archived {
            Some(true) => sql.push_str(" AND archived_at IS NOT NULL"),
            Some(false) => sql.push_str(" AND archived_at IS NULL"),
            None => {}
        }
        if let Some(kind) = query.kind {
            sql.push_str(" AND kind = ?");
            values.push(Value::Text(kind.as_str().to_owned()));
        }
        if let Some(repo_id) = &query.repo_id {
            sql.push_str(" AND repo_id = ?");
            values.push(Value::Text(repo_id.clone()));
        }
        if let Some(cursor) = &query.cursor {
            sql.push_str(&format!(" AND ({column} < ? OR ({column} = ? AND id < ?))"));
            values.push(Value::Text(cursor.value.clone()));
            values.push(Value::Text(cursor.value.clone()));
            values.push(Value::Text(cursor.id.clone()));
        }
        sql.push_str(&format!(" ORDER BY {column} DESC, id DESC LIMIT ?"));
        values.push(Value::Integer(i64::from(limit) + 1));

        self.database.with_connection(|connection| {
            let mut statement = connection.prepare(&sql)?;
            let rows = statement.query_map(params_from_iter(values.iter()), raw_summary)?;
            let mut captures = rows
                .map(|row| decode_summary(row?))
                .collect::<DbResult<Vec<_>>>()?;

            let has_more = captures.len() > limit as usize;
            if has_more {
                captures.truncate(limit as usize);
            }
            let next_cursor = if has_more {
                captures.last().map(|capture| {
                    let value = match query.sort {
                        CaptureSort::Updated => capture.updated_at.clone(),
                        CaptureSort::Created => capture.created_at.clone(),
                    };
                    CaptureCursor {
                        value,
                        id: capture.id.clone(),
                    }
                    .encode()
                })
            } else {
                None
            };

            Ok(CapturePage {
                captures,
                next_cursor,
                has_more,
            })
        })
    }

    /// 归档计数下沉 SQL，避免前端为拿角标而加载全部行。
    pub fn counts(&self, kind: Option<CaptureKind>) -> DbResult<CaptureCounts> {
        self.database.with_connection(|connection| {
            let mut sql = String::from(
                "SELECT COUNT(*),
                        COALESCE(SUM(CASE WHEN archived_at IS NULL THEN 1 ELSE 0 END), 0)
                 FROM captures",
            );
            let values: Vec<Value> = match kind {
                Some(kind) => {
                    sql.push_str(" WHERE kind = ?");
                    vec![Value::Text(kind.as_str().to_owned())]
                }
                None => Vec::new(),
            };

            let mut statement = connection.prepare(&sql)?;
            let (all, active) = statement.query_row(params_from_iter(values.iter()), |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
            })?;
            Ok(CaptureCounts {
                all,
                active,
                archived: all - active,
            })
        })
    }

    pub fn update(&self, capture: &Capture) -> DbResult<bool> {
        if capture.source.trim().is_empty() {
            return Err(DbError::InvalidData(
                "capture source cannot be empty".into(),
            ));
        }
        // body 只由同步或回源写入，用户编辑不动它；归档走 set_archived。
        self.database.with_connection(|connection| {
            let changed = connection.execute(
                "UPDATE captures
                 SET kind = ?2, source = ?3, url = ?4, title = ?5, summary = ?6,
                     note = ?7, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1",
                params![
                    capture.id,
                    capture.kind.as_str(),
                    capture.source,
                    capture.url,
                    capture.title,
                    capture.summary,
                    capture.note
                ],
            )?;
            Ok(changed > 0)
        })
    }

    /// 归档 / 取消归档。重复归档保留第一次的归档时间。
    pub fn set_archived(&self, id: &str, archived: bool) -> DbResult<bool> {
        self.database.with_connection(|connection| {
            let changed = connection.execute(
                "UPDATE captures
                 SET archived_at = CASE
                         WHEN ?2 = 1 THEN COALESCE(archived_at, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                         ELSE NULL
                     END,
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1",
                params![id, archived],
            )?;
            Ok(changed > 0)
        })
    }

    pub fn delete(&self, id: &str) -> DbResult<bool> {
        self.database.with_connection(|connection| {
            Ok(connection.execute("DELETE FROM captures WHERE id = ?1", [id])? > 0)
        })
    }

    pub fn search(&self, query: &str) -> DbResult<Vec<CaptureSummary>> {
        let query = fts_query(query);
        if query.is_empty() {
            return Ok(Vec::new());
        }
        self.database.with_connection(|connection| {
            let mut statement = connection.prepare(&format!(
                "SELECT {SEARCH_SUMMARY_FIELDS} FROM captures_fts
                 JOIN captures ON captures.rowid = captures_fts.rowid
                 WHERE captures_fts MATCH ?1
                 ORDER BY rank, captures.updated_at DESC
                 LIMIT ?2"
            ))?;
            let rows = statement.query_map(params![query, SEARCH_LIMIT], raw_summary)?;
            rows.map(|row| decode_summary(row?)).collect()
        })
    }

    pub fn get_body(&self, id: &str) -> DbResult<Option<CaptureBody>> {
        self.database.with_connection(|connection| {
            let result = connection
                .query_row(
                    "SELECT id, kind, url, body FROM captures WHERE id = ?1",
                    [id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, Option<String>>(3)?,
                        ))
                    },
                )
                .optional()?;
            result
                .map(|row| {
                    Ok(CaptureBody {
                        id: row.0,
                        kind: parse_value(row.1, "capture kind")?,
                        url: row.2,
                        body: row.3,
                    })
                })
                .transpose()
        })
    }

    /// 回源抓到的正文写回本地，下一次直接命中缓存。
    pub fn set_body(&self, id: &str, body: &str) -> DbResult<bool> {
        self.database.with_connection(|connection| {
            Ok(connection.execute(
                "UPDATE captures SET body = ?2 WHERE id = ?1",
                params![id, body],
            )? > 0)
        })
    }
}

fn get_capture(connection: &Connection, id: &str) -> DbResult<Option<Capture>> {
    let result = connection
        .query_row(
            &format!("SELECT {CAPTURE_FIELDS} FROM captures WHERE id = ?1"),
            [id],
            raw_capture,
        )
        .optional()?;
    result.map(decode_capture).transpose()
}

fn raw_capture(row: &Row<'_>) -> rusqlite::Result<RawCapture> {
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

fn raw_summary(row: &Row<'_>) -> rusqlite::Result<RawSummary> {
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
    ))
}

fn decode_capture(row: RawCapture) -> DbResult<Capture> {
    Ok(Capture {
        id: row.0,
        kind: parse_value(row.1, "capture kind")?,
        source: row.2,
        url: row.3,
        title: row.4,
        summary: row.5,
        note: row.6,
        body: row.7,
        repo_id: row.8,
        archived_at: row.9,
        created_at: row.10,
        updated_at: row.11,
    })
}

fn decode_summary(row: RawSummary) -> DbResult<CaptureSummary> {
    Ok(CaptureSummary {
        id: row.0,
        kind: parse_value(row.1, "capture kind")?,
        source: row.2,
        url: row.3,
        title: row.4,
        summary: row.5,
        note: row.6,
        repo_id: row.7,
        archived_at: row.8,
        created_at: row.9,
        updated_at: row.10,
    })
}

fn fts_query(query: &str) -> String {
    query
        .split_whitespace()
        .map(|word| format!("\"{}\"", word.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}
