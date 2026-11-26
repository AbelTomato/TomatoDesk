use rusqlite::{
    Connection, OptionalExtension, Row, Transaction, params, params_from_iter, types::Value,
};

use crate::{
    domain::{
        GithubThread, GithubThreadComment, ThreadDetail, ThreadPage, ThreadQuery,
        ThreadRelationship, ThreadSummary,
    },
    infrastructure::db::{Database, DbError, DbResult},
};

const THREAD_FIELDS: &str = "id, repo_id, kind, number, title, state, author, url, body, summary,
     is_author, is_assigned, review_requested, reviewed, comment_count, additions, deletions,
     changed_files, created_at, updated_at, closed_at, merged_at, synced_at";
const SUMMARY_FIELDS: &str = "id, repo_id, kind, number, title, state, author, url, summary,
     is_author, is_assigned, review_requested, reviewed, comment_count, additions, deletions,
     changed_files, created_at, updated_at, closed_at, merged_at";

pub struct GithubThreadRepository {
    database: Database,
}

impl GithubThreadRepository {
    pub(crate) fn new(database: Database) -> Self {
        Self { database }
    }

    /// 键集分页：一次多取一行判断 `hasMore`，与条目列表用同一套游标编码。
    pub fn page(&self, query: &ThreadQuery) -> DbResult<ThreadPage> {
        let limit = query.limit.clamp(1, 200);
        let mut sql = format!("SELECT {SUMMARY_FIELDS} FROM github_threads WHERE repo_id = ?");
        let mut values = vec![Value::Text(query.repo_id.clone())];

        if let Some(kind) = query.kind {
            sql.push_str(" AND kind = ?");
            values.push(Value::Text(kind.as_str().to_owned()));
        }
        if let Some(state) = query.state {
            // 「打开中」把草稿也算进去：草稿 PR 是打开但未就绪。
            if state.is_open() {
                sql.push_str(" AND state IN ('open', 'draft')");
            } else {
                sql.push_str(" AND state = ?");
                values.push(Value::Text(state.as_str().to_owned()));
            }
        }
        if let Some(relationship) = query.relationship {
            sql.push_str(&format!(" AND {}", relationship_filter(relationship)));
        }
        if let Some(cursor) = &query.cursor {
            sql.push_str(" AND (updated_at < ? OR (updated_at = ? AND id < ?))");
            values.push(Value::Text(cursor.value.clone()));
            values.push(Value::Text(cursor.value.clone()));
            values.push(Value::Text(cursor.id.clone()));
        }
        sql.push_str(" ORDER BY updated_at DESC, id DESC LIMIT ?");
        values.push(Value::Integer(i64::from(limit) + 1));

        self.database.with_connection(|connection| {
            let mut statement = connection.prepare(&sql)?;
            let rows = statement.query_map(params_from_iter(values.iter()), map_summary)?;
            let mut threads = rows
                .map(|row| row.map_err(DbError::from))
                .collect::<DbResult<Vec<_>>>()?;

            let has_more = threads.len() > limit as usize;
            if has_more {
                threads.truncate(limit as usize);
            }
            let next_cursor = if has_more {
                threads
                    .last()
                    .map(|thread| format!("{}|{}", thread.updated_at, thread.id))
            } else {
                None
            };
            Ok(ThreadPage {
                threads,
                next_cursor,
                has_more,
            })
        })
    }

    /// 通知只带 PR / Issue 编号，用 `(repo_id, number)` 找回本地线程 id。
    /// 未同步过的线程返回 `None`，收件箱此时只存外部链接。
    pub fn find_id(&self, repo_id: &str, number: i64) -> DbResult<Option<String>> {
        self.database.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT id FROM github_threads WHERE repo_id = ?1 AND number = ?2",
                    params![repo_id, number],
                    |row| row.get(0),
                )
                .optional()
                .map_err(Into::into)
        })
    }

    /// 详情：正文与本地评论只在这里取，列表不携带。
    pub fn get(&self, id: &str) -> DbResult<Option<ThreadDetail>> {
        self.database.with_connection(|connection| {
            let result = connection
                .query_row(
                    &format!(
                        "SELECT {THREAD_FIELDS}, comments_synced_at
                         FROM github_threads WHERE id = ?1"
                    ),
                    [id],
                    |row| {
                        let thread = map_thread(row)?;
                        let comments_synced_at: Option<String> = row.get("comments_synced_at")?;
                        Ok((thread, comments_synced_at))
                    },
                )
                .optional()?;
            result
                .map(|(thread, comments_synced_at)| {
                    Ok(ThreadDetail {
                        body: thread.body.clone(),
                        summary: ThreadSummary::from(&thread),
                        comments: comments_for(connection, id)?,
                        comments_synced_at,
                    })
                })
                .transpose()
        })
    }

    /// 整批替换某个线程的评论：GitHub 侧删除的评论会在下一次刷新时消失。
    pub fn replace_comments(
        &self,
        thread_id: &str,
        comments: &[GithubThreadComment],
        synced_at: &str,
    ) -> DbResult<()> {
        self.database.with_transaction(|transaction| {
            transaction.execute(
                "DELETE FROM github_thread_comments WHERE thread_id = ?1",
                [thread_id],
            )?;
            for comment in comments {
                transaction.execute(
                    "INSERT INTO github_thread_comments(
                         id, thread_id, github_id, author, body, url,
                         created_at, updated_at, synced_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        comment.id,
                        comment.thread_id,
                        comment.github_id,
                        comment.author,
                        comment.body,
                        comment.url,
                        comment.created_at,
                        comment.updated_at,
                        comment.synced_at
                    ],
                )?;
            }
            transaction.execute(
                "UPDATE github_threads SET comments_synced_at = ?2 WHERE id = ?1",
                params![thread_id, synced_at],
            )?;
            Ok(())
        })
    }
}

/// 评论按创建时间升序；同一时间戳用 GitHub 的 id 兜底，保证顺序稳定。
fn comments_for(connection: &Connection, thread_id: &str) -> DbResult<Vec<GithubThreadComment>> {
    let mut statement = connection.prepare(
        "SELECT id, thread_id, github_id, author, body, url, created_at, updated_at, synced_at
         FROM github_thread_comments
         WHERE thread_id = ?1
         ORDER BY created_at ASC, github_id ASC",
    )?;
    let rows = statement.query_map([thread_id], map_comment)?;
    rows.map(|row| row.map_err(DbError::from)).collect()
}

fn relationship_filter(relationship: ThreadRelationship) -> &'static str {
    match relationship {
        ThreadRelationship::Author => "is_author = 1",
        ThreadRelationship::Assigned => "is_assigned = 1",
        ThreadRelationship::ReviewRequested => "review_requested = 1",
        ThreadRelationship::Reviewed => "reviewed = 1",
        ThreadRelationship::None => {
            "is_author = 0 AND is_assigned = 0 AND review_requested = 0 AND reviewed = 0"
        }
    }
}

fn map_thread(row: &Row<'_>) -> rusqlite::Result<GithubThread> {
    Ok(GithubThread {
        id: row.get("id")?,
        repo_id: row.get("repo_id")?,
        kind: parse_column(row, "kind")?,
        number: row.get("number")?,
        title: row.get("title")?,
        state: parse_column(row, "state")?,
        author: row.get("author")?,
        url: row.get("url")?,
        body: row.get("body")?,
        summary: row.get("summary")?,
        is_author: row.get("is_author")?,
        is_assigned: row.get("is_assigned")?,
        review_requested: row.get("review_requested")?,
        reviewed: row.get("reviewed")?,
        comment_count: row.get("comment_count")?,
        additions: row.get("additions")?,
        deletions: row.get("deletions")?,
        changed_files: row.get("changed_files")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        closed_at: row.get("closed_at")?,
        merged_at: row.get("merged_at")?,
        synced_at: row.get("synced_at")?,
    })
}

fn map_summary(row: &Row<'_>) -> rusqlite::Result<ThreadSummary> {
    Ok(ThreadSummary {
        id: row.get("id")?,
        repo_id: row.get("repo_id")?,
        kind: parse_column(row, "kind")?,
        number: row.get("number")?,
        title: row.get("title")?,
        state: parse_column(row, "state")?,
        author: row.get("author")?,
        url: row.get("url")?,
        summary: row.get("summary")?,
        is_author: row.get("is_author")?,
        is_assigned: row.get("is_assigned")?,
        review_requested: row.get("review_requested")?,
        reviewed: row.get("reviewed")?,
        comment_count: row.get("comment_count")?,
        additions: row.get("additions")?,
        deletions: row.get("deletions")?,
        changed_files: row.get("changed_files")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        closed_at: row.get("closed_at")?,
        merged_at: row.get("merged_at")?,
    })
}

fn map_comment(row: &Row<'_>) -> rusqlite::Result<GithubThreadComment> {
    Ok(GithubThreadComment {
        id: row.get("id")?,
        thread_id: row.get("thread_id")?,
        github_id: row.get("github_id")?,
        author: row.get("author")?,
        body: row.get("body")?,
        url: row.get("url")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        synced_at: row.get("synced_at")?,
    })
}

fn parse_column<T>(row: &Row<'_>, column: &str) -> rusqlite::Result<T>
where
    T: std::str::FromStr<Err = String>,
{
    let raw: String = row.get(column)?;
    raw.parse().map_err(|error| {
        rusqlite::Error::InvalidColumnType(
            0,
            format!("{column}: {error}"),
            rusqlite::types::Type::Text,
        )
    })
}

/// 线程 upsert：内容没变时不写库，返回值就是「新增或变化」的行数。
pub(super) fn upsert_threads(
    transaction: &Transaction<'_>,
    threads: &[GithubThread],
) -> DbResult<usize> {
    let mut changed_total = 0;
    for thread in threads {
        if thread.title.trim().is_empty() || thread.url.trim().is_empty() {
            return Err(DbError::InvalidData(
                "GitHub thread title and url cannot be empty".into(),
            ));
        }
        let changed = transaction.execute(
            "INSERT INTO github_threads(
                 id, repo_id, kind, number, title, state, author, url, body, summary,
                 is_author, is_assigned, review_requested, reviewed, comment_count,
                 additions, deletions, changed_files, created_at, updated_at,
                 closed_at, merged_at, synced_at
             ) VALUES (
                 ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                 ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23
             )
             ON CONFLICT(repo_id, kind, number) DO UPDATE SET
                 title = excluded.title,
                 state = excluded.state,
                 author = excluded.author,
                 url = excluded.url,
                 body = excluded.body,
                 summary = excluded.summary,
                 is_author = excluded.is_author,
                 is_assigned = excluded.is_assigned,
                 review_requested = excluded.review_requested,
                 reviewed = excluded.reviewed,
                 comment_count = excluded.comment_count,
                 additions = excluded.additions,
                 deletions = excluded.deletions,
                 changed_files = excluded.changed_files,
                 updated_at = excluded.updated_at,
                 closed_at = excluded.closed_at,
                 merged_at = excluded.merged_at,
                 synced_at = excluded.synced_at
             WHERE github_threads.title IS NOT excluded.title
                OR github_threads.state IS NOT excluded.state
                OR github_threads.body IS NOT excluded.body
                OR github_threads.is_author IS NOT excluded.is_author
                OR github_threads.is_assigned IS NOT excluded.is_assigned
                OR github_threads.review_requested IS NOT excluded.review_requested
                OR github_threads.reviewed IS NOT excluded.reviewed
                OR github_threads.comment_count IS NOT excluded.comment_count
                OR github_threads.additions IS NOT excluded.additions
                OR github_threads.deletions IS NOT excluded.deletions
                OR github_threads.changed_files IS NOT excluded.changed_files
                OR github_threads.updated_at IS NOT excluded.updated_at
                OR github_threads.closed_at IS NOT excluded.closed_at
                OR github_threads.merged_at IS NOT excluded.merged_at",
            params![
                thread.id,
                thread.repo_id,
                thread.kind.as_str(),
                thread.number,
                thread.title,
                thread.state.as_str(),
                thread.author,
                thread.url,
                thread.body,
                thread.summary,
                thread.is_author,
                thread.is_assigned,
                thread.review_requested,
                thread.reviewed,
                thread.comment_count,
                thread.additions,
                thread.deletions,
                thread.changed_files,
                thread.created_at,
                thread.updated_at,
                thread.closed_at,
                thread.merged_at,
                thread.synced_at
            ],
        )?;
        changed_total += changed;
    }
    Ok(changed_total)
}

#[cfg(test)]
mod tests {
    use super::relationship_filter;
    use crate::domain::ThreadRelationship;

    #[test]
    fn relationship_filters_are_null_safe_and_exclusive() {
        assert_eq!(
            relationship_filter(ThreadRelationship::ReviewRequested),
            "review_requested = 1"
        );
        assert!(
            relationship_filter(ThreadRelationship::None).contains("is_author = 0"),
            "none must exclude every relation"
        );
    }
}
