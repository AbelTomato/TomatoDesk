use rusqlite::{Connection, OptionalExtension, Row, params, params_from_iter, types::Value};
use uuid::Uuid;

use crate::{
    domain::{
        Item, ItemBody, ItemCursor, ItemKind, ItemPage, ItemPageQuery, ItemSort, ItemStatus,
        ItemStatusCounts, ItemSummary, NewItem,
    },
    infrastructure::db::{Database, DbError, DbResult},
};

use super::parse_value;

type RawItem = (
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
    String,
    String,
    String,
);

const ITEM_FIELDS: &str =
    "id, kind, source, url, title, summary, note, body, repo_id, status, created_at, updated_at";
const SUMMARY_FIELDS: &str =
    "id, kind, source, url, title, summary, note, repo_id, status, created_at, updated_at";
const SEARCH_SUMMARY_FIELDS: &str = "items.id, items.kind, items.source, items.url, items.title,
                                     items.summary, items.note, items.repo_id, items.status,
                                     items.created_at, items.updated_at";
const SEARCH_LIMIT: u32 = 100;

pub struct ItemRepository {
    database: Database,
}

impl ItemRepository {
    pub(crate) fn new(database: Database) -> Self {
        Self { database }
    }

    pub fn create(&self, item: NewItem) -> DbResult<Item> {
        if item.source.trim().is_empty() {
            return Err(DbError::InvalidData("item source cannot be empty".into()));
        }
        let id = Uuid::new_v4().to_string();
        self.database.with_connection(|connection| {
            connection.execute(
                "INSERT INTO items(id, kind, source, url, title, summary, note, body, repo_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    id,
                    item.kind.as_str(),
                    item.source,
                    item.url,
                    item.title,
                    item.summary,
                    item.note,
                    item.body,
                    item.repo_id
                ],
            )?;
            get_item(connection, &id)?.ok_or(DbError::NotFound("item"))
        })
    }

    pub fn get(&self, id: &str) -> DbResult<Option<Item>> {
        self.database
            .with_connection(|connection| get_item(connection, id))
    }

    pub fn list(&self) -> DbResult<Vec<Item>> {
        self.database.with_connection(|connection| {
            let mut statement = connection.prepare(&format!(
                "SELECT {ITEM_FIELDS} FROM items ORDER BY updated_at DESC, id DESC"
            ))?;
            let rows = statement.query_map([], raw_item)?;
            rows.map(|row| decode_item(row?)).collect()
        })
    }

    /// 键集分页：一次多取一行用于判断是否还有下一页，避免额外的 count 查询。
    pub fn page(&self, query: &ItemPageQuery) -> DbResult<ItemPage> {
        let limit = query.limit.clamp(1, 200);
        let column = query.sort.column();
        let mut sql = format!("SELECT {SUMMARY_FIELDS} FROM items WHERE 1 = 1");
        let mut values: Vec<Value> = Vec::new();

        if let Some(status) = query.status {
            sql.push_str(" AND status = ?");
            values.push(Value::Text(status.as_str().to_owned()));
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
            let mut items = rows
                .map(|row| decode_summary(row?))
                .collect::<DbResult<Vec<_>>>()?;

            let has_more = items.len() > limit as usize;
            if has_more {
                items.truncate(limit as usize);
            }
            let next_cursor = if has_more {
                items.last().map(|item| {
                    let value = match query.sort {
                        ItemSort::Updated => item.updated_at.clone(),
                        ItemSort::Created => item.created_at.clone(),
                    };
                    ItemCursor {
                        value,
                        id: item.id.clone(),
                    }
                    .encode()
                })
            } else {
                None
            };

            Ok(ItemPage {
                items,
                next_cursor,
                has_more,
            })
        })
    }

    /// 状态计数下沉 SQL，避免前端为拿角标而加载全部行。
    pub fn count_by_status(&self, kind: Option<ItemKind>) -> DbResult<ItemStatusCounts> {
        self.database.with_connection(|connection| {
            let mut sql = String::from("SELECT status, COUNT(*) FROM items");
            let values: Vec<Value> = match kind {
                Some(kind) => {
                    sql.push_str(" WHERE kind = ?");
                    vec![Value::Text(kind.as_str().to_owned())]
                }
                None => Vec::new(),
            };
            sql.push_str(" GROUP BY status");

            let mut statement = connection.prepare(&sql)?;
            let rows = statement.query_map(params_from_iter(values.iter()), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;

            let mut counts = ItemStatusCounts::default();
            for row in rows {
                let (status, count) = row?;
                counts.all += count;
                match parse_value::<ItemStatus>(status, "item status")? {
                    ItemStatus::Pending => counts.pending = count,
                    ItemStatus::Processed => counts.processed = count,
                    ItemStatus::Archived => counts.archived = count,
                }
            }
            Ok(counts)
        })
    }

    pub fn update(&self, item: &Item) -> DbResult<bool> {
        if item.source.trim().is_empty() {
            return Err(DbError::InvalidData("item source cannot be empty".into()));
        }
        // body 只由同步或回源写入，用户编辑不动它。
        self.database.with_connection(|connection| {
            let changed = connection.execute(
                "UPDATE items
                 SET kind = ?2, source = ?3, url = ?4, title = ?5, summary = ?6,
                     note = ?7, status = ?8,
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1",
                params![
                    item.id,
                    item.kind.as_str(),
                    item.source,
                    item.url,
                    item.title,
                    item.summary,
                    item.note,
                    item.status.as_str()
                ],
            )?;
            Ok(changed > 0)
        })
    }

    pub fn delete(&self, id: &str) -> DbResult<bool> {
        self.database.with_connection(|connection| {
            Ok(connection.execute("DELETE FROM items WHERE id = ?1", [id])? > 0)
        })
    }

    pub fn search(&self, query: &str) -> DbResult<Vec<ItemSummary>> {
        let query = fts_query(query);
        if query.is_empty() {
            return Ok(Vec::new());
        }
        self.database.with_connection(|connection| {
            let mut statement = connection.prepare(&format!(
                "SELECT {SEARCH_SUMMARY_FIELDS} FROM items_fts
                 JOIN items ON items.rowid = items_fts.rowid
                 WHERE items_fts MATCH ?1
                 ORDER BY rank, items.updated_at DESC
                 LIMIT ?2"
            ))?;
            let rows = statement.query_map(params![query, SEARCH_LIMIT], raw_summary)?;
            rows.map(|row| decode_summary(row?)).collect()
        })
    }

    pub fn get_body(&self, id: &str) -> DbResult<Option<ItemBody>> {
        self.database.with_connection(|connection| {
            let result = connection
                .query_row(
                    "SELECT id, kind, url, body FROM items WHERE id = ?1",
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
                    Ok(ItemBody {
                        id: row.0,
                        kind: parse_value(row.1, "item kind")?,
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
                "UPDATE items SET body = ?2 WHERE id = ?1",
                params![id, body],
            )? > 0)
        })
    }
}

fn get_item(connection: &Connection, id: &str) -> DbResult<Option<Item>> {
    let result = connection
        .query_row(
            &format!("SELECT {ITEM_FIELDS} FROM items WHERE id = ?1"),
            [id],
            raw_item,
        )
        .optional()?;
    result.map(decode_item).transpose()
}

fn raw_item(row: &Row<'_>) -> rusqlite::Result<RawItem> {
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

fn decode_item(row: RawItem) -> DbResult<Item> {
    Ok(Item {
        id: row.0,
        kind: parse_value(row.1, "item kind")?,
        source: row.2,
        url: row.3,
        title: row.4,
        summary: row.5,
        note: row.6,
        body: row.7,
        repo_id: row.8,
        status: parse_value(row.9, "item status")?,
        created_at: row.10,
        updated_at: row.11,
    })
}

fn decode_summary(row: RawSummary) -> DbResult<ItemSummary> {
    Ok(ItemSummary {
        id: row.0,
        kind: parse_value(row.1, "item kind")?,
        source: row.2,
        url: row.3,
        title: row.4,
        summary: row.5,
        note: row.6,
        repo_id: row.7,
        status: parse_value(row.8, "item status")?,
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
