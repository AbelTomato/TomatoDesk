use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

use rusqlite::{Connection, Transaction};

use super::{
    DbError, DbResult, GithubEventRepository, GithubRepoRepository, ItemRepository,
    SyncRunRepository, TagRepository,
};

struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "initial_schema",
        sql: include_str!("../../../migrations/0001_initial.sql"),
    },
    Migration {
        version: 2,
        name: "item_body",
        sql: include_str!("../../../migrations/0002_item_body.sql"),
    },
];

#[derive(Clone)]
pub struct Database {
    connection: Arc<Mutex<Connection>>,
    path: Option<PathBuf>,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> DbResult<Self> {
        let path = path.as_ref();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }

        let connection = Connection::open(path)?;
        let database = Self {
            connection: Arc::new(Mutex::new(connection)),
            path: Some(path.to_path_buf()),
        };
        database.migrate()?;
        Ok(database)
    }

    pub fn in_memory() -> DbResult<Self> {
        let database = Self {
            connection: Arc::new(Mutex::new(Connection::open_in_memory()?)),
            path: None,
        };
        database.migrate()?;
        Ok(database)
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn migrate(&self) -> DbResult<()> {
        self.with_connection_mut(|connection| {
            connection.execute_batch(
                "PRAGMA foreign_keys = ON;
                 CREATE TABLE IF NOT EXISTS schema_migrations (
                     version INTEGER PRIMARY KEY,
                     name TEXT NOT NULL,
                     applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                 );",
            )?;

            for migration in MIGRATIONS {
                let applied: bool = connection.query_row(
                    "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?1)",
                    [migration.version],
                    |row| row.get(0),
                )?;
                if !applied {
                    let transaction = connection.transaction()?;
                    transaction.execute_batch(migration.sql)?;
                    transaction.execute(
                        "INSERT INTO schema_migrations(version, name) VALUES (?1, ?2)",
                        (migration.version, migration.name),
                    )?;
                    transaction.commit()?;
                }
            }
            Ok(())
        })
    }

    pub fn items(&self) -> ItemRepository {
        ItemRepository::new(self.clone())
    }

    pub fn tags(&self) -> TagRepository {
        TagRepository::new(self.clone())
    }

    pub fn github_repos(&self) -> GithubRepoRepository {
        GithubRepoRepository::new(self.clone())
    }

    pub fn github_events(&self) -> GithubEventRepository {
        GithubEventRepository::new(self.clone())
    }

    pub fn sync_runs(&self) -> SyncRunRepository {
        SyncRunRepository::new(self.clone())
    }

    pub(super) fn connection(&self) -> DbResult<MutexGuard<'_, Connection>> {
        self.connection.lock().map_err(|_| DbError::LockPoisoned)
    }

    pub(super) fn with_connection<T>(
        &self,
        operation: impl FnOnce(&Connection) -> DbResult<T>,
    ) -> DbResult<T> {
        let connection = self.connection()?;
        operation(&connection)
    }

    pub(super) fn with_connection_mut<T>(
        &self,
        operation: impl FnOnce(&mut Connection) -> DbResult<T>,
    ) -> DbResult<T> {
        let mut connection = self.connection()?;
        operation(&mut connection)
    }

    pub(super) fn with_transaction<T>(
        &self,
        operation: impl FnOnce(&Transaction<'_>) -> DbResult<T>,
    ) -> DbResult<T> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let value = operation(&transaction)?;
        transaction.commit()?;
        Ok(value)
    }
}
