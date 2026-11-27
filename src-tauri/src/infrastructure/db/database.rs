use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

use rusqlite::{Connection, Transaction};

use super::{
    CaptureRepository, DbError, DbResult, GithubReleaseRepository, GithubRepoRepository,
    GithubThreadRepository, InboxMessageRepository, InboxReadBackRepository, InboxSourceRepository,
    SyncRunRepository, TagRepository, TaskRepository,
};

struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
    /// 迁移完成后展示给用户的说明；没有用户可见变化时为 `None`。
    message: Option<&'static str>,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "initial_schema",
        sql: include_str!("../../../migrations/0001_initial.sql"),
        message: None,
    },
    Migration {
        version: 2,
        name: "item_body",
        sql: include_str!("../../../migrations/0002_item_body.sql"),
        message: None,
    },
    Migration {
        version: 3,
        name: "repo_workspace",
        sql: include_str!("../../../migrations/0003_repo_workspace.sql"),
        message: None,
    },
    Migration {
        version: 4,
        name: "thread_comments",
        sql: include_str!("../../../migrations/0004_thread_comments.sql"),
        message: None,
    },
    Migration {
        version: 5,
        name: "capture_module",
        sql: include_str!("../../../migrations/0005_capture_module.sql"),
        message: Some("收集已独立为「随手记」：原来的「已处理」状态已取消，相关条目归为未归档。"),
    },
    Migration {
        version: 6,
        name: "tasks",
        sql: include_str!("../../../migrations/0006_tasks.sql"),
        message: None,
    },
    Migration {
        version: 7,
        name: "inbox_module",
        sql: include_str!("../../../migrations/0007_inbox_module.sql"),
        message: None,
    },
    Migration {
        version: 8,
        name: "inbox_read_back",
        sql: include_str!("../../../migrations/0008_inbox_read_back.sql"),
        message: None,
    },
    Migration {
        version: 9,
        name: "inbox_v06",
        sql: include_str!("../../../migrations/0009_inbox_v06.sql"),
        message: None,
    },
];

/// 本次启动执行了迁移时留下的痕迹，供前端提示。
#[derive(Debug, Clone, Default)]
struct MigrationNotice {
    backup_path: Option<PathBuf>,
    migrated_to: Option<i64>,
    message: Option<String>,
}

#[derive(Clone)]
pub struct Database {
    connection: Arc<Mutex<Connection>>,
    path: Option<PathBuf>,
    notice: Arc<Mutex<MigrationNotice>>,
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
            notice: Arc::new(Mutex::new(MigrationNotice::default())),
        };
        database.migrate()?;
        Ok(database)
    }

    pub fn in_memory() -> DbResult<Self> {
        let database = Self {
            connection: Arc::new(Mutex::new(Connection::open_in_memory()?)),
            path: None,
            notice: Arc::new(Mutex::new(MigrationNotice::default())),
        };
        database.migrate()?;
        Ok(database)
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// 本次启动因迁移产生的备份文件；没有执行迁移时为 `None`。
    pub fn migration_backup(&self) -> Option<PathBuf> {
        self.notice
            .lock()
            .ok()
            .and_then(|notice| notice.backup_path.clone())
    }

    /// 本次启动把库迁移到的最高版本；没有执行迁移时为 `None`。
    pub fn migrated_to(&self) -> Option<i64> {
        self.notice
            .lock()
            .ok()
            .and_then(|notice| notice.migrated_to)
    }

    /// 本次迁移对应的用户提示；没有提示时为 `None`。
    pub fn migration_message(&self) -> Option<String> {
        self.notice
            .lock()
            .ok()
            .and_then(|notice| notice.message.clone())
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

            let applied = {
                let mut statement = connection.prepare("SELECT version FROM schema_migrations")?;
                let rows = statement.query_map([], |row| row.get::<_, i64>(0))?;
                rows.collect::<Result<HashSet<i64>, _>>()?
            };
            let pending = MIGRATIONS
                .iter()
                .filter(|migration| !applied.contains(&migration.version))
                .collect::<Vec<_>>();
            if pending.is_empty() {
                return Ok(());
            }

            // 已有数据的库先备份：0003 会删除旧 GitHub 条目，迁移失败也需要能回到原库。
            let backup = if applied.is_empty() {
                None
            } else {
                self.backup_database(connection, applied.iter().copied().max().unwrap_or(0))?
            };

            for migration in &pending {
                let transaction = connection.transaction()?;
                transaction.execute_batch(migration.sql)?;
                transaction.execute(
                    "INSERT INTO schema_migrations(version, name) VALUES (?1, ?2)",
                    (migration.version, migration.name),
                )?;
                transaction.commit()?;
            }

            if let Ok(mut notice) = self.notice.lock() {
                notice.backup_path = backup;
                notice.migrated_to = pending.iter().map(|migration| migration.version).max();
                notice.message = pending
                    .iter()
                    .rev()
                    .find_map(|migration| migration.message)
                    .map(ToOwned::to_owned);
            }
            Ok(())
        })
    }

    /// 复制一份当前数据库；内存库没有可备份的文件。
    /// 优先用 `VACUUM INTO`，它产出的副本一致且不携带日志文件。
    fn backup_database(
        &self,
        connection: &Connection,
        from_version: i64,
    ) -> DbResult<Option<PathBuf>> {
        let Some(path) = self.path.as_deref() else {
            return Ok(None);
        };
        let directory = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .join("backup");
        fs::create_dir_all(&directory)?;

        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let target = directory.join(format!("tomatodesk-v{from_version}-{stamp}.sqlite3"));
        let target_text = target.to_string_lossy().into_owned();
        if connection
            .execute("VACUUM INTO ?1", [&target_text])
            .is_err()
        {
            fs::copy(path, &target)?;
        }
        Ok(Some(target))
    }

    pub fn captures(&self) -> CaptureRepository {
        CaptureRepository::new(self.clone())
    }

    pub fn tags(&self) -> TagRepository {
        TagRepository::new(self.clone())
    }

    pub fn github_repos(&self) -> GithubRepoRepository {
        GithubRepoRepository::new(self.clone())
    }

    pub fn github_threads(&self) -> GithubThreadRepository {
        GithubThreadRepository::new(self.clone())
    }

    pub fn github_releases(&self) -> GithubReleaseRepository {
        GithubReleaseRepository::new(self.clone())
    }

    pub fn sync_runs(&self) -> SyncRunRepository {
        SyncRunRepository::new(self.clone())
    }

    pub fn tasks(&self) -> TaskRepository {
        TaskRepository::new(self.clone())
    }

    pub fn inbox_sources(&self) -> InboxSourceRepository {
        InboxSourceRepository::new(self.clone())
    }

    pub fn inbox_messages(&self) -> InboxMessageRepository {
        InboxMessageRepository::new(self.clone())
    }

    pub fn inbox_read_back(&self) -> InboxReadBackRepository {
        InboxReadBackRepository::new(self.clone())
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
