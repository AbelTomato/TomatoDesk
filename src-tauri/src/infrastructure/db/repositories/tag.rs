use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use uuid::Uuid;

use crate::{
    domain::Tag,
    infrastructure::db::{Database, DbError, DbResult},
};

const TAG_FIELDS: &str = "id, name, color, created_at";

pub struct TagRepository {
    database: Database,
}

impl TagRepository {
    pub(crate) fn new(database: Database) -> Self {
        Self { database }
    }

    pub fn create(&self, name: &str, color: Option<&str>) -> DbResult<Tag> {
        let name = name.trim();
        if name.is_empty() {
            return Err(DbError::InvalidData("tag name cannot be empty".into()));
        }
        let id = Uuid::new_v4().to_string();
        self.database.with_connection(|connection| {
            connection.execute(
                "INSERT INTO tags(id, name, color) VALUES (?1, ?2, ?3)",
                params![id, name, color],
            )?;
            get_tag(connection, &id)?.ok_or(DbError::NotFound("tag"))
        })
    }

    pub fn get(&self, id: &str) -> DbResult<Option<Tag>> {
        self.database
            .with_connection(|connection| get_tag(connection, id))
    }

    pub fn list(&self) -> DbResult<Vec<Tag>> {
        self.database.with_connection(|connection| {
            let mut statement = connection.prepare(&format!(
                "SELECT {TAG_FIELDS} FROM tags ORDER BY name COLLATE NOCASE, id"
            ))?;
            let rows = statement.query_map([], map_tag)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
        })
    }

    pub fn delete(&self, id: &str) -> DbResult<bool> {
        self.database.with_connection(|connection| {
            Ok(connection.execute("DELETE FROM tags WHERE id = ?1", [id])? > 0)
        })
    }

    pub fn set_capture_tags(&self, capture_id: &str, tag_ids: &[String]) -> DbResult<()> {
        let unique_tags = tag_ids.iter().collect::<HashSet<_>>();
        self.database.with_transaction(|transaction| {
            ensure_capture_exists(transaction, capture_id)?;
            transaction.execute(
                "DELETE FROM capture_tags WHERE capture_id = ?1",
                [capture_id],
            )?;
            for tag_id in unique_tags {
                transaction.execute(
                    "INSERT INTO capture_tags(capture_id, tag_id) VALUES (?1, ?2)",
                    params![capture_id, tag_id],
                )?;
            }
            Ok(())
        })
    }

    pub fn for_capture(&self, capture_id: &str) -> DbResult<Vec<Tag>> {
        self.database.with_connection(|connection| {
            let mut statement = connection.prepare(&format!(
                "SELECT {TAG_FIELDS}
                 FROM tags JOIN capture_tags ON capture_tags.tag_id = tags.id
                 WHERE capture_tags.capture_id = ?1
                 ORDER BY tags.name COLLATE NOCASE, tags.id"
            ))?;
            let rows = statement.query_map([capture_id], map_tag)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
        })
    }
}

fn get_tag(connection: &Connection, id: &str) -> DbResult<Option<Tag>> {
    connection
        .query_row(
            &format!("SELECT {TAG_FIELDS} FROM tags WHERE id = ?1"),
            [id],
            map_tag,
        )
        .optional()
        .map_err(Into::into)
}

fn map_tag(row: &Row<'_>) -> rusqlite::Result<Tag> {
    Ok(Tag {
        id: row.get(0)?,
        name: row.get(1)?,
        color: row.get(2)?,
        created_at: row.get(3)?,
    })
}

fn ensure_capture_exists(transaction: &Transaction<'_>, id: &str) -> DbResult<()> {
    let exists: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM captures WHERE id = ?1)",
        [id],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(DbError::NotFound("capture"));
    }
    Ok(())
}
