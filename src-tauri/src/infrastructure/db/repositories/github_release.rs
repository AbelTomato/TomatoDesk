use rusqlite::{Row, Transaction, params};

use crate::{
    domain::GithubRelease,
    infrastructure::db::{Database, DbError, DbResult},
};

const RELEASE_FIELDS: &str =
    "id, repo_id, tag_name, name, url, body, summary, is_draft, is_prerelease,
     published_at, created_at, synced_at";

pub struct GithubReleaseRepository {
    database: Database,
}

impl GithubReleaseRepository {
    pub(crate) fn new(database: Database) -> Self {
        Self { database }
    }

    pub fn list_for_repo(&self, repo_id: &str) -> DbResult<Vec<GithubRelease>> {
        self.database.with_connection(|connection| {
            let mut statement = connection.prepare(&format!(
                "SELECT {RELEASE_FIELDS} FROM github_releases
                 WHERE repo_id = ?1
                 ORDER BY COALESCE(published_at, created_at) DESC, tag_name DESC"
            ))?;
            let rows = statement.query_map([repo_id], map_release)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
        })
    }
}

fn map_release(row: &Row<'_>) -> rusqlite::Result<GithubRelease> {
    Ok(GithubRelease {
        id: row.get("id")?,
        repo_id: row.get("repo_id")?,
        tag_name: row.get("tag_name")?,
        name: row.get("name")?,
        url: row.get("url")?,
        body: row.get("body")?,
        summary: row.get("summary")?,
        is_draft: row.get("is_draft")?,
        is_prerelease: row.get("is_prerelease")?,
        published_at: row.get("published_at")?,
        created_at: row.get("created_at")?,
        synced_at: row.get("synced_at")?,
    })
}

/// Release upsert：正文没变时不写库，返回值是「新增或变化」的行数。
pub(super) fn upsert_releases(
    transaction: &Transaction<'_>,
    releases: &[GithubRelease],
) -> DbResult<usize> {
    let mut changed_total = 0;
    for release in releases {
        if release.tag_name.trim().is_empty() || release.url.trim().is_empty() {
            return Err(DbError::InvalidData(
                "GitHub release tag and url cannot be empty".into(),
            ));
        }
        let changed = transaction.execute(
            "INSERT INTO github_releases(
                 id, repo_id, tag_name, name, url, body, summary,
                 is_draft, is_prerelease, published_at, created_at, synced_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(repo_id, tag_name) DO UPDATE SET
                 name = excluded.name,
                 url = excluded.url,
                 body = excluded.body,
                 summary = excluded.summary,
                 is_draft = excluded.is_draft,
                 is_prerelease = excluded.is_prerelease,
                 published_at = excluded.published_at,
                 synced_at = excluded.synced_at
             WHERE github_releases.name IS NOT excluded.name
                OR github_releases.body IS NOT excluded.body
                OR github_releases.is_draft IS NOT excluded.is_draft
                OR github_releases.is_prerelease IS NOT excluded.is_prerelease
                OR github_releases.published_at IS NOT excluded.published_at",
            params![
                release.id,
                release.repo_id,
                release.tag_name,
                release.name,
                release.url,
                release.body,
                release.summary,
                release.is_draft,
                release.is_prerelease,
                release.published_at,
                release.created_at,
                release.synced_at
            ],
        )?;
        changed_total += changed;
    }
    Ok(changed_total)
}
