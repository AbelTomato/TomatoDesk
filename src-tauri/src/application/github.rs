use std::{collections::HashMap, time::Duration};

use chrono::{SecondsFormat, Utc};
use tauri::Emitter;
use uuid::Uuid;

use crate::{
    domain::{
        GithubRelease, GithubRepo, GithubThread, NewSyncRun, README_TEXT_MAX_CHARS, SyncRunStatus,
        ThreadKind, ThreadState, excerpt, readme_summary,
    },
    infrastructure::{
        db::Database,
        github::{GithubClient, GithubIssue, GithubPull},
        secrets,
    },
};

const SYNC_SOURCE: &str = "github";
/// 每个仓库单次同步最多补查这么多个 PR 的评审记录，避免把请求数放大到不可控。
const REVIEW_LOOKUP_LIMIT: usize = 20;

/// 增量同步只看上次同步后更新过的内容；`Full` 是显式的「加载历史」，会拉全部状态。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncScope {
    #[default]
    Incremental,
    Full,
}

impl SyncScope {
    /// 有增量游标时用 `all`：刚关闭的条目也能被正确标记为 closed。
    /// 首次同步没有游标，只取 `open`，避免一次拉回全部历史。
    fn state(self, since: Option<&str>) -> &'static str {
        match self {
            Self::Full => "all",
            Self::Incremental if since.is_some() => "all",
            Self::Incremental => "open",
        }
    }

    /// 只有增量同步带 `since` 游标；全量同步必须忽略它，否则拿不到历史数据。
    fn uses_cursor(self) -> bool {
        self == Self::Incremental
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSummary {
    pub repo_id: String,
    pub threads_count: usize,
    pub releases_count: usize,
    /// 新增或内容发生变化的行数；0 即视为「内容没有变化」。
    pub changed_count: usize,
    pub not_modified: bool,
    pub synced_at: String,
}

/// 四条关系相对当前 Token 用户，可以同时成立。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ThreadRelations {
    pub is_author: bool,
    pub is_assigned: bool,
    pub review_requested: bool,
    pub reviewed: bool,
}

pub async fn sync_github_repo(
    database: &Database,
    repo_id: &str,
    token: &str,
    scope: SyncScope,
) -> Result<SyncSummary, String> {
    let repo = database
        .github_repos()
        .get(repo_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "GitHub 仓库不存在".to_owned())?;
    let client = GithubClient::new(token.to_owned(), Duration::from_secs(20))
        .map_err(|error| error.to_string())?;
    let viewer = client.viewer().await.map_err(|error| error.to_string())?;
    run_sync(database, repo, &client, &viewer.login, scope).await
}

pub async fn sync_all_github_repos(database: &Database) -> Result<Vec<SyncSummary>, String> {
    let token = secrets::get_github_token()?.ok_or_else(|| "请先保存 GitHub Token".to_owned())?;
    let client =
        GithubClient::new(token, Duration::from_secs(20)).map_err(|error| error.to_string())?;
    let viewer = client.viewer().await.map_err(|error| error.to_string())?;
    let repos = database
        .github_repos()
        .list()
        .map_err(|error| error.to_string())?;

    let mut summaries = Vec::new();
    let mut errors = Vec::new();
    for repo in repos.into_iter().filter(|repo| repo.enabled) {
        match run_sync(
            database,
            repo.clone(),
            &client,
            &viewer.login,
            SyncScope::Incremental,
        )
        .await
        {
            Ok(summary) => summaries.push(summary),
            Err(error) => errors.push(format!("{}/{}：{error}", repo.owner, repo.name)),
        }
    }
    if errors.is_empty() {
        Ok(summaries)
    } else {
        Err(errors.join("；"))
    }
}

pub fn start_scheduler(database: Database, app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30 * 60));
        interval.tick().await;
        loop {
            interval.tick().await;
            if secrets::get_github_token().ok().flatten().is_some() {
                let result = sync_all_github_repos(&database).await;
                let _ = app.emit("github-sync-updated", ());
                match result {
                    Ok(summaries) if !summaries.is_empty() => {
                        let changed = summaries
                            .iter()
                            .map(|summary| summary.changed_count)
                            .sum::<usize>();
                        let body = format!(
                            "检查 {} 个仓库，{} 条内容有更新。",
                            summaries.len(),
                            changed
                        );
                        let _ = crate::platform::show_notification(&app, "GitHub 同步完成", &body);
                    }
                    Ok(_) => {}
                    Err(error) => {
                        let _ = crate::platform::show_notification(&app, "GitHub 同步失败", &error);
                    }
                }
            }
        }
    });
}

/// 一次仓库同步：记录 sync_run，失败时把错误写回运行记录。
async fn run_sync(
    database: &Database,
    repo: GithubRepo,
    client: &GithubClient,
    viewer: &str,
    scope: SyncScope,
) -> Result<SyncSummary, String> {
    let started_at = now_iso();
    let mut run = database
        .sync_runs()
        .create(NewSyncRun {
            source: SYNC_SOURCE.into(),
            started_at,
        })
        .map_err(|error| error.to_string())?;

    match sync_repo_contents(database, &repo, client, viewer, scope).await {
        Ok(summary) => {
            run.finished_at = Some(summary.synced_at.clone());
            run.status = SyncRunStatus::Succeeded;
            run.error = None;
            database
                .sync_runs()
                .update(&run)
                .map_err(|error| error.to_string())?;
            Ok(summary)
        }
        Err(error) => {
            run.finished_at = Some(now_iso());
            run.status = SyncRunStatus::Failed;
            run.error = Some(error.clone());
            let _ = database.sync_runs().update(&run);
            Err(error)
        }
    }
}

async fn sync_repo_contents(
    database: &Database,
    repo: &GithubRepo,
    client: &GithubClient,
    viewer: &str,
    scope: SyncScope,
) -> Result<SyncSummary, String> {
    // `since` 每次同步都会前进，同一 URL 的 ETag 无法复用，因此不再发条件请求；
    // 增量本身已经把载荷压到「上次同步后更新过的内容」。
    let since = scope
        .uses_cursor()
        .then(|| repo.last_synced_at.clone())
        .flatten();
    let state = scope.state(since.as_deref());
    let synced_at = now_iso();

    let meta = client
        .repository(&repo.owner, &repo.name)
        .await
        .map_err(|error| error.to_string())?;
    // README 拿不到不影响同步：失败时保留上一次的内容，仓库真的没有 README 才清空。
    let readme = client.readme(&repo.owner, &repo.name).await;
    let (readme_text, readme_summary, readme_synced_at) = match &readme {
        Ok(text) => (
            text.as_deref()
                .map(|value| value.chars().take(README_TEXT_MAX_CHARS).collect()),
            text.as_deref().and_then(readme_summary),
            Some(synced_at.clone()),
        ),
        Err(_) => (
            repo.readme_text.clone(),
            repo.readme_summary.clone(),
            repo.readme_synced_at.clone(),
        ),
    };
    let releases = client
        .releases(&repo.owner, &repo.name)
        .await
        .map_err(|error| error.to_string())?;
    let pulls = client
        .pulls(&repo.owner, &repo.name, state, since.as_deref())
        .await
        .map_err(|error| error.to_string())?;
    // issues 接口会把 PR 一起返回，这里只保留真正的 Issue。
    let issues = client
        .issues(&repo.owner, &repo.name, state, since.as_deref())
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|issue| issue.pull_request.is_none())
        .collect::<Vec<_>>();

    let review_authors = collect_review_authors(client, repo, viewer, &pulls).await;

    let mut threads = Vec::with_capacity(pulls.len() + issues.len());
    for pull in &pulls {
        let relations = infer_relations(
            viewer,
            pull.user.as_ref().map(|user| user.login.as_str()),
            &logins(pull.assignees.as_deref()),
            &logins(pull.requested_reviewers.as_deref()),
            &review_authors
                .get(&pull.number)
                .map(|logins| logins.iter().map(String::as_str).collect::<Vec<_>>())
                .unwrap_or_default(),
        );
        threads.push(pull_thread(&repo.id, pull, relations, &synced_at));
    }
    for issue in &issues {
        let relations = infer_relations(
            viewer,
            issue.user.as_ref().map(|user| user.login.as_str()),
            &logins(issue.assignees.as_deref()),
            &[],
            &[],
        );
        threads.push(issue_thread(&repo.id, issue, relations, &synced_at));
    }

    let releases = releases
        .iter()
        .map(|release| GithubRelease {
            id: Uuid::new_v4().to_string(),
            repo_id: repo.id.clone(),
            tag_name: release.tag_name.clone(),
            name: release.name.clone(),
            url: release.html_url.clone(),
            body: release.body.clone(),
            summary: release.body.as_deref().and_then(excerpt),
            is_draft: release.draft,
            is_prerelease: release.prerelease,
            published_at: release.published_at.clone(),
            created_at: release.created_at.clone(),
            synced_at: synced_at.clone(),
        })
        .collect::<Vec<_>>();

    let mut updated_repo = repo.clone();
    updated_repo.description = meta.description;
    updated_repo.homepage = meta.homepage;
    updated_repo.language = meta.language;
    updated_repo.stars = meta.stargazers_count;
    updated_repo.forks = meta.forks_count;
    updated_repo.open_issues = meta.open_issues_count;
    updated_repo.default_branch = meta.default_branch;
    updated_repo.pushed_at = meta.pushed_at;
    updated_repo.is_archived = meta.archived;
    updated_repo.is_private = meta.private;
    updated_repo.readme_text = readme_text;
    updated_repo.readme_summary = readme_summary;
    updated_repo.readme_synced_at = readme_synced_at;
    updated_repo.meta_synced_at = Some(synced_at.clone());
    updated_repo.last_synced_at = Some(synced_at.clone());

    // 仓库元信息、线程、Release 共用一个事务，失败时整批回滚。
    let writes = database
        .github_repos()
        .apply_sync(&updated_repo, &threads, &releases)
        .map_err(|error| error.to_string())?;

    let changed_count = writes.threads_upserted + writes.releases_upserted;
    Ok(SyncSummary {
        repo_id: repo.id.clone(),
        threads_count: threads.len(),
        releases_count: releases.len(),
        changed_count,
        not_modified: changed_count == 0,
        synced_at,
    })
}

/// 补查「我评审过」：只看不是我作者、也没要求我评审的 PR，按最近更新取前若干个。
async fn collect_review_authors(
    client: &GithubClient,
    repo: &GithubRepo,
    viewer: &str,
    pulls: &[GithubPull],
) -> HashMap<u64, Vec<String>> {
    let mut result = HashMap::new();
    let candidates = pulls
        .iter()
        .filter(|pull| {
            let is_author = pull
                .user
                .as_ref()
                .is_some_and(|user| user.login.eq_ignore_ascii_case(viewer));
            let requested = logins(pull.requested_reviewers.as_deref())
                .iter()
                .any(|login| login.eq_ignore_ascii_case(viewer));
            !is_author && !requested
        })
        .take(REVIEW_LOOKUP_LIMIT);

    for pull in candidates {
        let Ok(reviews) = client.reviews(&repo.owner, &repo.name, pull.number).await else {
            continue;
        };
        let mut logins = reviews
            .iter()
            .filter_map(|review| review.user.as_ref().map(|user| user.login.clone()))
            .collect::<Vec<_>>();
        logins.sort();
        logins.dedup();
        if !logins.is_empty() {
            result.insert(pull.number, logins);
        }
    }
    result
}

fn pull_thread(
    repo_id: &str,
    pull: &GithubPull,
    relations: ThreadRelations,
    synced_at: &str,
) -> GithubThread {
    let state = if pull.merged_at.is_some() {
        ThreadState::Merged
    } else if pull.state == "open" && pull.draft {
        ThreadState::Draft
    } else if pull.state == "open" {
        ThreadState::Open
    } else {
        ThreadState::Closed
    };
    GithubThread {
        id: Uuid::new_v4().to_string(),
        repo_id: repo_id.to_owned(),
        kind: ThreadKind::PullRequest,
        number: pull.number as i64,
        title: pull.title.clone(),
        state,
        author: pull.user.as_ref().map(|user| user.login.clone()),
        url: pull.html_url.clone(),
        body: pull.body.clone(),
        summary: pull.body.as_deref().and_then(excerpt),
        is_author: relations.is_author,
        is_assigned: relations.is_assigned,
        review_requested: relations.review_requested,
        reviewed: relations.reviewed,
        comment_count: pull.comments.unwrap_or(0) as i64,
        additions: pull.additions,
        deletions: pull.deletions,
        changed_files: pull.changed_files,
        created_at: pull.created_at.clone(),
        updated_at: pull.updated_at.clone(),
        closed_at: pull.closed_at.clone(),
        merged_at: pull.merged_at.clone(),
        synced_at: synced_at.to_owned(),
    }
}

fn issue_thread(
    repo_id: &str,
    issue: &GithubIssue,
    relations: ThreadRelations,
    synced_at: &str,
) -> GithubThread {
    GithubThread {
        id: Uuid::new_v4().to_string(),
        repo_id: repo_id.to_owned(),
        kind: ThreadKind::Issue,
        number: issue.number as i64,
        title: issue.title.clone(),
        state: if issue.state == "open" {
            ThreadState::Open
        } else {
            ThreadState::Closed
        },
        author: issue.user.as_ref().map(|user| user.login.clone()),
        url: issue.html_url.clone(),
        body: issue.body.clone(),
        summary: issue.body.as_deref().and_then(excerpt),
        is_author: relations.is_author,
        is_assigned: relations.is_assigned,
        review_requested: false,
        reviewed: false,
        comment_count: issue.comments.unwrap_or(0) as i64,
        additions: None,
        deletions: None,
        changed_files: None,
        created_at: issue.created_at.clone(),
        updated_at: issue.updated_at.clone(),
        closed_at: issue.closed_at.clone(),
        merged_at: None,
        synced_at: synced_at.to_owned(),
    }
}

fn logins(users: Option<&[crate::infrastructure::github::GithubUser]>) -> Vec<&str> {
    users
        .unwrap_or_default()
        .iter()
        .map(|user| user.login.as_str())
        .collect()
}

pub(crate) fn infer_relations(
    viewer: &str,
    author: Option<&str>,
    assignees: &[&str],
    requested_reviewers: &[&str],
    review_authors: &[&str],
) -> ThreadRelations {
    let matches = |logins: &[&str]| {
        logins
            .iter()
            .any(|login| login.eq_ignore_ascii_case(viewer))
    };
    ThreadRelations {
        is_author: author.is_some_and(|login| login.eq_ignore_ascii_case(viewer)),
        is_assigned: matches(assignees),
        review_requested: matches(requested_reviewers),
        reviewed: matches(review_authors),
    }
}

fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use super::{SyncScope, ThreadRelations, infer_relations};

    #[test]
    fn relations_cover_author_assignee_review_request_and_reviewed() {
        assert_eq!(
            infer_relations("abel", Some("Abel"), &[], &[], &[]),
            ThreadRelations {
                is_author: true,
                ..ThreadRelations::default()
            },
            "author match should be case insensitive"
        );
        assert_eq!(
            infer_relations("abel", Some("someone"), &["abel"], &[], &[]),
            ThreadRelations {
                is_assigned: true,
                ..ThreadRelations::default()
            }
        );
        assert_eq!(
            infer_relations("abel", Some("someone"), &[], &["abel"], &[]),
            ThreadRelations {
                review_requested: true,
                ..ThreadRelations::default()
            }
        );
        assert_eq!(
            infer_relations("abel", Some("someone"), &[], &[], &["other", "abel"]),
            ThreadRelations {
                reviewed: true,
                ..ThreadRelations::default()
            }
        );
        assert_eq!(
            infer_relations("abel", Some("someone"), &["other"], &[], &[]),
            ThreadRelations::default(),
            "unrelated threads keep every flag false"
        );
    }

    #[test]
    fn incremental_sync_uses_open_only_without_a_cursor() {
        assert_eq!(SyncScope::Incremental.state(None), "open");
        assert_eq!(
            SyncScope::Incremental.state(Some("2026-11-25T00:00:00Z")),
            "all",
            "a cursor lets closed threads be marked correctly"
        );
        assert_eq!(SyncScope::Full.state(None), "all");
        assert!(!SyncScope::Full.uses_cursor());
    }
}
