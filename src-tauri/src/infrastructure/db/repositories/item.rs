use rusqlite::{Connection, OptionalExtension, Row, params};
use uuid::Uuid;

use crate::{
    domain::{Item, NewItem},
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
    String,
    String,
    String,
);

const ITEM_FIELDS: &str =
    "id, kind, source, url, title, summary, note, status, created_at, updated_at";
const SEARCH_ITEMS_SQL: &str = "SELECT items.id, items.kind, items.source, items.url, items.title,
                                items.summary, items.note, items.status, items.created_at, items.updated_at
                         FROM items_fts JOIN items ON items.rowid = items_fts.rowid
                         WHERE items_fts MATCH ?1
                         ORDER BY rank, items.created_at DESC";

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
                "INSERT INTO items(id, kind, source, url, title, summary, note)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    id,
                    item.kind.as_str(),
                    item.source,
                    item.url,
                    item.title,
                    item.summary,
                    item.note
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
                "SELECT {ITEM_FIELDS} FROM items ORDER BY created_at DESC, id"
            ))?;
            let rows = statement.query_map([], raw_item)?;
            rows.map(|row| decode_item(row?)).collect()
        })
    }

    pub fn update(&self, item: &Item) -> DbResult<bool> {
        if item.source.trim().is_empty() {
            return Err(DbError::InvalidData("item source cannot be empty".into()));
        }
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

    pub fn search(&self, query: &str) -> DbResult<Vec<Item>> {
        let query = fts_query(query);
        if query.is_empty() {
            return Ok(Vec::new());
        }
        self.database.with_connection(|connection| {
            let mut statement = connection.prepare(SEARCH_ITEMS_SQL)?;
            let rows = statement.query_map([query], raw_item)?;
            rows.map(|row| decode_item(row?)).collect()
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
        status: parse_value(row.7, "item status")?,
        created_at: row.8,
        updated_at: row.9,
    })
}

fn fts_query(query: &str) -> String {
    query
        .split_whitespace()
        .map(|word| format!("\"{}\"", word.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}
