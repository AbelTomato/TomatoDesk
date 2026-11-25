use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension, Row, params};
use uuid::Uuid;

use crate::{
    domain::{
        GithubRelease, GithubRepo, GithubThread, RepoDetail, RepoSummary, RepoThreadCounts,
        SyncWrites,
    },
    infrastructure::db::{Database, DbError, DbResult},
};

const REPO_FIELDS: &str = "id, owner, name, enabled, last_synced_at, etag, description, homepage,
     language, stars, forks, open_issues, default_branch, pushed_at, is_archived, is_private,
     readme_text, readme_summary, readme_synced_at, custom_description, custom_note,
     meta_synced_at, created_at, updated_at";

/// 未关闭线程的聚合：一次查询覆盖列表页所有仓库的角标。
const THREAD_COUNT_SQL: &str = "SELECT repo_id,
        COALESCE(SUM(CASE WHEN kind = 'pull_request' AND state IN ('open', 'draft')
            THEN 1 ELSE 0 END), 0),
        COALESCE(SUM(CASE WHEN kind = 'issue' AND state = 'open' THEN 1 ELSE 0 END), 0),
        COALESCE(SUM(CASE WHEN kind = 'pull_request' AND state IN ('open', 'draft')
            AND (review_requested = 1 OR reviewed = 1) THEN 1 ELSE 0 END), 0)
     FROM github_threads GROUP BY repo_id";

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
                "SELECT {REPO_FIELDS} FROM github_repos
                 ORDER BY owner COLLATE NOCASE, name COLLATE NOCASE"
            ))?;
            let rows = statement.query_map([], map_repo)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
        })
    }

    /// 仓库列表：元信息 + 未关闭线程数 + 我的条目数，全部由 SQL 聚合。
    pub fn list_summaries(&self) -> DbResult<Vec<RepoSummary>> {
        let repos = self.list()?;
        self.database.with_connection(|connection| {
            let thread_counts = grouped_counts(connection, THREAD_COUNT_SQL, |row| {
                Ok((row.get(1)?, row.get(2)?, row.get(3)?))
            })?;
            let release_counts = grouped_totals(
                connection,
                "SELECT repo_id, COUNT(*) FROM github_releases GROUP BY repo_id",
            )?;
            let item_counts = grouped_totals(
                connection,
                "SELECT repo_id, COUNT(*) FROM items WHERE repo_id IS NOT NULL GROUP BY repo_id",
            )?;

            Ok(repos
                .into_iter()
                .map(|repo| {
                    let (pull_requests, issues, reviews) =
                        thread_counts.get(&repo.id).copied().unwrap_or((0, 0, 0));
                    RepoSummary {
                        description: repo.display_description().map(ToOwned::to_owned),
                        language: repo.language.clone(),
                        stars: repo.stars,
                        open_pull_requests: pull_requests,
                        open_issues: issues,
                        reviews,
                        releases: release_counts.get(&repo.id).copied().unwrap_or(0),
                        items: item_counts.get(&repo.id).copied().unwrap_or(0),
                        enabled: repo.enabled,
                        last_synced_at: repo.last_synced_at.clone(),
                        is_archived: repo.is_archived,
                        is_private: repo.is_private,
                        id: repo.id,
                        owner: repo.owner,
                        name: repo.name,
                    }
                })
                .collect())
        })
    }

    pub fn detail(&self, id: &str) -> DbResult<Option<RepoDetail>> {
        let Some(repo) = self.get(id)? else {
            return Ok(None);
        };
        let counts = self.counts(id)?;
        Ok(Some(RepoDetail { repo, counts }))
    }

    pub fn counts(&self, repo_id: &str) -> DbResult<RepoThreadCounts> {
        self.database.with_connection(|connection| {
            let (pull_requests, issues, reviews) = connection.query_row(
                "SELECT
                    COALESCE(SUM(CASE WHEN kind = 'pull_request'
                        AND state IN ('open', 'draft') THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE WHEN kind = 'issue' AND state = 'open'
                        THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE WHEN kind = 'pull_request'
                        AND state IN ('open', 'draft')
                        AND (review_requested = 1 OR reviewed = 1) THEN 1 ELSE 0 END), 0)
                 FROM github_threads WHERE repo_id = ?1",
                [repo_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
            let releases = count_rows(connection, "github_releases", repo_id)?;
            let items = count_rows(connection, "items", repo_id)?;
            Ok(RepoThreadCounts {
                open_pull_requests: pull_requests,
                open_issues: issues,
                reviews,
                releases,
                items,
            })
        })
    }

    /// 用户自定义描述与备注，只有这两个字段允许手改。
    pub fn update_meta(
        &self,
        id: &str,
        custom_description: Option<&str>,
        custom_note: Option<&str>,
    ) -> DbResult<bool> {
        self.database.with_connection(|connection| {
            Ok(connection.execute(
                "UPDATE github_repos
                 SET custom_description = ?2, custom_note = ?3,
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1",
                params![id, custom_description, custom_note],
            )? > 0)
        })
    }

    /// 一次同步的全部写入合并为一个事务：仓库元信息、线程、Release。
    /// 失败时整批回滚，不会留下半截数据。
    pub fn apply_sync(
        &self,
        repo: &GithubRepo,
        threads: &[GithubThread],
        releases: &[GithubRelease],
    ) -> DbResult<SyncWrites> {
        self.database.with_transaction(|transaction| {
            let threads_upserted = super::github_thread::upsert_threads(transaction, threads)?;
            let releases_upserted = super::github_release::upsert_releases(transaction, releases)?;
            let changed = transaction.execute(
                "UPDATE github_repos
                 SET description = ?2, homepage = ?3, language = ?4, stars = ?5, forks = ?6,
                     open_issues = ?7, default_branch = ?8, pushed_at = ?9, is_archived = ?10,
                     is_private = ?11, readme_text = ?12, readme_summary = ?13,
                     readme_synced_at = ?14, meta_synced_at = ?15, last_synced_at = ?16,
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1",
                params![
                    repo.id,
                    repo.description,
                    repo.homepage,
                    repo.language,
                    repo.stars,
                    repo.forks,
                    repo.open_issues,
                    repo.default_branch,
                    repo.pushed_at,
                    repo.is_archived,
                    repo.is_private,
                    repo.readme_text,
                    repo.readme_summary,
                    repo.readme_synced_at,
                    repo.meta_synced_at,
                    repo.last_synced_at
                ],
            )?;
            if changed == 0 {
                return Err(DbError::NotFound("GitHub repository"));
            }
            Ok(SyncWrites {
                threads_upserted,
                releases_upserted,
            })
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
        description: row.get(6)?,
        homepage: row.get(7)?,
        language: row.get(8)?,
        stars: row.get(9)?,
        forks: row.get(10)?,
        open_issues: row.get(11)?,
        default_branch: row.get(12)?,
        pushed_at: row.get(13)?,
        is_archived: row.get(14)?,
        is_private: row.get(15)?,
        readme_text: row.get(16)?,
        readme_summary: row.get(17)?,
        readme_synced_at: row.get(18)?,
        custom_description: row.get(19)?,
        custom_note: row.get(20)?,
        meta_synced_at: row.get(21)?,
        created_at: row.get(22)?,
        updated_at: row.get(23)?,
    })
}

fn count_rows(connection: &Connection, table: &str, repo_id: &str) -> DbResult<i64> {
    connection
        .query_row(
            &format!("SELECT COUNT(*) FROM {table} WHERE repo_id = ?1"),
            [repo_id],
            |row| row.get(0),
        )
        .map_err(Into::into)
}

fn grouped_counts<T>(
    connection: &Connection,
    sql: &str,
    decode: impl Fn(&Row<'_>) -> rusqlite::Result<T>,
) -> DbResult<HashMap<String, T>> {
    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map([], |row| Ok((row.get::<_, String>(0)?, decode(row)?)))?;
    let mut map = HashMap::new();
    for row in rows {
        let (repo_id, value) = row?;
        map.insert(repo_id, value);
    }
    Ok(map)
}

fn grouped_totals(connection: &Connection, sql: &str) -> DbResult<HashMap<String, i64>> {
    grouped_counts(connection, sql, |row| row.get(1))
}
