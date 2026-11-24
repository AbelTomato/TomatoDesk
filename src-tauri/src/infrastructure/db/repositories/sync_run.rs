use rusqlite::{Connection, OptionalExtension, Row, params};
use uuid::Uuid;

use crate::{
    domain::{NewSyncRun, SyncRun, SyncRunStatus},
    infrastructure::db::{Database, DbResult},
};

use super::parse_value;

type RawSyncRun = (
    String,
    String,
    String,
    Option<String>,
    String,
    Option<String>,
);

const SYNC_RUN_FIELDS: &str = "id, source, started_at, finished_at, status, error";

pub struct SyncRunRepository {
    database: Database,
}

impl SyncRunRepository {
    pub(crate) fn new(database: Database) -> Self {
        Self { database }
    }

    pub fn create(&self, run: NewSyncRun) -> DbResult<SyncRun> {
        let id = Uuid::new_v4().to_string();
        self.database.with_connection(|connection| {
            connection.execute(
                "INSERT INTO sync_runs(id, source, started_at) VALUES (?1, ?2, ?3)",
                params![id, run.source, run.started_at],
            )?;
            get_sync_run(connection, &id)?
                .ok_or(crate::infrastructure::db::DbError::NotFound("sync run"))
        })
    }

    pub fn get(&self, id: &str) -> DbResult<Option<SyncRun>> {
        self.database
            .with_connection(|connection| get_sync_run(connection, id))
    }

    pub fn list_latest(&self, source: &str, limit: u32) -> DbResult<Vec<SyncRun>> {
        self.database.with_connection(|connection| {
            let mut statement = connection.prepare(&format!(
                "SELECT {SYNC_RUN_FIELDS} FROM sync_runs
                 WHERE source = ?1 ORDER BY started_at DESC, id LIMIT ?2"
            ))?;
            let rows = statement.query_map(params![source, limit], raw_sync_run)?;
            rows.map(|row| decode_sync_run(row?)).collect()
        })
    }

    pub fn update(&self, run: &SyncRun) -> DbResult<bool> {
        self.database.with_connection(|connection| {
            let changed = connection.execute(
                "UPDATE sync_runs
                 SET finished_at = ?2, status = ?3, error = ?4
                 WHERE id = ?1",
                params![run.id, run.finished_at, run.status.as_str(), run.error],
            )?;
            Ok(changed > 0)
        })
    }
}

fn get_sync_run(connection: &Connection, id: &str) -> DbResult<Option<SyncRun>> {
    connection
        .query_row(
            &format!("SELECT {SYNC_RUN_FIELDS} FROM sync_runs WHERE id = ?1"),
            [id],
            raw_sync_run,
        )
        .optional()?
        .map(decode_sync_run)
        .transpose()
}

fn raw_sync_run(row: &Row<'_>) -> rusqlite::Result<RawSyncRun> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
    ))
}

fn decode_sync_run(row: RawSyncRun) -> DbResult<SyncRun> {
    Ok(SyncRun {
        id: row.0,
        source: row.1,
        started_at: row.2,
        finished_at: row.3,
        status: parse_value::<SyncRunStatus>(row.4, "sync run status")?,
        error: row.5,
    })
}
