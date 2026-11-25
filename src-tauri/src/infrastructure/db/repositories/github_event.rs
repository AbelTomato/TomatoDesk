use rusqlite::{Row, params};
use uuid::Uuid;

use crate::{
    domain::{GithubEvent, GithubEventType, NewGithubEvent, NewItem},
    infrastructure::db::{Database, DbError, DbResult},
};

use super::parse_value;

type RawEvent = (String, String, String, String, String, String, String);

const EVENT_FIELDS: &str =
    "id, repo_id, source_event_id, event_type, payload_json, occurred_at, created_at";

pub struct GithubEventRepository {
    database: Database,
}

impl GithubEventRepository {
    pub(crate) fn new(database: Database) -> Self {
        Self { database }
    }

    pub fn insert(&self, event: &NewGithubEvent) -> DbResult<bool> {
        if event.repo_id.trim().is_empty() || event.source_event_id.trim().is_empty() {
            return Err(DbError::InvalidData(
                "GitHub event repository and source identifiers cannot be empty".into(),
            ));
        }
        serde_json::from_str::<serde_json::Value>(&event.payload_json)
            .map_err(|error| DbError::InvalidData(format!("invalid GitHub event JSON: {error}")))?;

        let id = Uuid::new_v4().to_string();
        self.database.with_connection(|connection| {
            let changed = connection.execute(
                "INSERT INTO github_events(id, repo_id, source_event_id, event_type, payload_json, occurred_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(repo_id, source_event_id) DO NOTHING",
                params![
                    id,
                    event.repo_id,
                    event.source_event_id,
                    event.event_type.as_str(),
                    event.payload_json,
                    event.occurred_at
                ],
            )?;
            Ok(changed > 0)
        })
    }

    /// 单仓库一次同步的全部写入合并进一个事务：
    /// 事件去重、条目落库、正文只写一份，失败时整批回滚。
    pub fn insert_with_items(&self, entries: &[(NewGithubEvent, NewItem)]) -> DbResult<usize> {
        for (event, item) in entries {
            if event.repo_id.trim().is_empty() || event.source_event_id.trim().is_empty() {
                return Err(DbError::InvalidData(
                    "GitHub event repository and source identifiers cannot be empty".into(),
                ));
            }
            if item.source.trim().is_empty() {
                return Err(DbError::InvalidData("item source cannot be empty".into()));
            }
            serde_json::from_str::<serde_json::Value>(&event.payload_json).map_err(|error| {
                DbError::InvalidData(format!("invalid GitHub event JSON: {error}"))
            })?;
        }

        self.database.with_transaction(|transaction| {
            let mut inserted = 0;
            for (event, item) in entries {
                let changed = transaction.execute(
                    "INSERT INTO github_events(id, repo_id, source_event_id, event_type, payload_json, occurred_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT(repo_id, source_event_id) DO NOTHING",
                    params![
                        Uuid::new_v4().to_string(),
                        event.repo_id,
                        event.source_event_id,
                        event.event_type.as_str(),
                        event.payload_json,
                        event.occurred_at
                    ],
                )?;
                if changed == 0 {
                    continue;
                }
                transaction.execute(
                    "INSERT INTO items(id, kind, source, url, title, summary, note, body)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        Uuid::new_v4().to_string(),
                        item.kind.as_str(),
                        item.source,
                        item.url,
                        item.title,
                        item.summary,
                        item.note,
                        item.body
                    ],
                )?;
                inserted += 1;
            }
            Ok(inserted)
        })
    }

    pub fn list_for_repo(&self, repo_id: &str) -> DbResult<Vec<GithubEvent>> {
        self.database.with_connection(|connection| {
            let mut statement = connection.prepare(&format!(
                "SELECT {EVENT_FIELDS} FROM github_events
                 WHERE repo_id = ?1 ORDER BY occurred_at DESC, id"
            ))?;
            let rows = statement.query_map([repo_id], raw_event)?;
            rows.map(|row| decode_event(row?)).collect()
        })
    }
}

fn raw_event(row: &Row<'_>) -> rusqlite::Result<RawEvent> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
    ))
}

fn decode_event(row: RawEvent) -> DbResult<GithubEvent> {
    Ok(GithubEvent {
        id: row.0,
        repo_id: row.1,
        source_event_id: row.2,
        event_type: parse_value::<GithubEventType>(row.3, "GitHub event type")?,
        payload_json: row.4,
        occurred_at: row.5,
        created_at: row.6,
    })
}
