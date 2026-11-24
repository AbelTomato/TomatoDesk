use rusqlite::{Connection, OptionalExtension, Row, params};
use uuid::Uuid;

use crate::{
    domain::GithubRepo,
    infrastructure::db::{Database, DbError, DbResult},
};

const REPO_FIELDS: &str = "id, owner, name, enabled, last_synced_at, etag, created_at, updated_at";

pub struct GithubRepoRepository {
    database: Database,
}

impl GithubRepoRepository {
    pub(crate) fn new(database: Database) -> Self {
        Self { database }
    }

    pub fn create(&self, owner: &str, name: &str) -> DbResult<GithubRepo> {
        let owner = owner.trim();
        let name = name.trim();
        if owner.is_empty() || name.is_empty() {
            return Err(DbError::InvalidData(
                "GitHub repository owner and name cannot be empty".into(),
            ));
        }
        let id = Uuid::new_v4().to_string();
        self.database.with_connection(|connection| {
            connection.execute(
                "INSERT INTO github_repos(id, owner, name) VALUES (?1, ?2, ?3)",
                params![id, owner, name],
            )?;
            get_repo(connection, &id)?.ok_or(DbError::NotFound("GitHub repository"))
        })
    }

    pub fn get(&self, id: &str) -> DbResult<Option<GithubRepo>> {
        self.database
            .with_connection(|connection| get_repo(connection, id))
    }

    pub fn list(&self) -> DbResult<Vec<GithubRepo>> {
        self.database.with_connection(|connection| {
            let mut statement = connection.prepare(&format!(
                "SELECT {REPO_FIELDS} FROM github_repos ORDER BY owner COLLATE NOCASE, name COLLATE NOCASE"
            ))?;
            let rows = statement.query_map([], map_repo)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
        })
    }

    pub fn update(&self, repo: &GithubRepo) -> DbResult<bool> {
        self.database.with_connection(|connection| {
            let changed = connection.execute(
                "UPDATE github_repos
                 SET enabled = ?2, last_synced_at = ?3, etag = ?4,
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1",
                params![repo.id, repo.enabled, repo.last_synced_at, repo.etag],
            )?;
            Ok(changed > 0)
        })
    }

    pub fn delete(&self, id: &str) -> DbResult<bool> {
        self.database.with_connection(|connection| {
            Ok(connection.execute("DELETE FROM github_repos WHERE id = ?1", [id])? > 0)
        })
    }
}

fn get_repo(connection: &Connection, id: &str) -> DbResult<Option<GithubRepo>> {
    connection
        .query_row(
            &format!("SELECT {REPO_FIELDS} FROM github_repos WHERE id = ?1"),
            [id],
            map_repo,
        )
        .optional()
        .map_err(Into::into)
}

fn map_repo(row: &Row<'_>) -> rusqlite::Result<GithubRepo> {
    Ok(GithubRepo {
        id: row.get(0)?,
        owner: row.get(1)?,
        name: row.get(2)?,
        enabled: row.get(3)?,
        last_synced_at: row.get(4)?,
        etag: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}
