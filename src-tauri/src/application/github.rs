use std::time::Duration;

use chrono::{SecondsFormat, Utc};
use tauri::Emitter;

use crate::{
    domain::{
        GithubEventType, ItemKind, NewGithubEvent, NewItem, NewSyncRun, SyncRunStatus, excerpt,
    },
    infrastructure::{db::Database, github::GithubClient, secrets},
};

const SYNC_SOURCE: &str = "github";

/// 增量同步只看未关闭内容；`Full` 是显式的「加载历史」，会拉全部状态且忽略游标。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncScope {
    #[default]
    Incremental,
    Full,
}

impl SyncScope {
    fn state(self) -> &'static str {
        match self {
            Self::Incremental => "open",
            Self::Full => "all",
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
    pub imported_count: usize,
    pub releases_count: usize,
    pub issues_count: usize,
    pub not_modified: bool,
    pub synced_at: String,
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
    let started_at = now_iso();
    let mut run = database
        .sync_runs()
        .create(NewSyncRun {
            source: SYNC_SOURCE.into(),
            started_at,
        })
        .map_err(|error| error.to_string())?;

    let result = sync_repo_contents(database, &repo, token, scope).await;
    match result {
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

pub async fn sync_all_github_repos(database: &Database) -> Result<Vec<SyncSummary>, String> {
    let token = secrets::get_github_token()?.ok_or_else(|| "请先保存 GitHub Token".to_owned())?;
    let repos = database
        .github_repos()
        .list()
        .map_err(|error| error.to_string())?;
    let mut summaries = Vec::new();
    let mut errors = Vec::new();
    for repo in repos.into_iter().filter(|repo| repo.enabled) {
        match sync_github_repo(database, &repo.id, &token, SyncScope::Incremental).await {
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
                        let imported = summaries
                            .iter()
                            .map(|summary| summary.imported_count)
                            .sum::<usize>();
                        let body = format!(
                            "检查 {} 个仓库，新增 {} 条更新。",
                            summaries.len(),
                            imported
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

async fn sync_repo_contents(
    database: &Database,
    repo: &crate::domain::GithubRepo,
    token: &str,
    scope: SyncScope,
) -> Result<SyncSummary, String> {
    let client = GithubClient::new(token.to_owned(), Duration::from_secs(20))
        .map_err(|error| error.to_string())?;
    // `since` 每次同步都会前进，同一 URL 的 ETag 无法复用，因此不再发条件请求；
    // 增量本身已经把载荷压到「上次同步后更新过的内容」。
    let since = scope
        .uses_cursor()
        .then(|| repo.last_synced_at.clone())
        .flatten();

    let releases = client
        .releases(&repo.owner, &repo.name, None)
        .await
        .map_err(|error| error.to_string())?;
    let issues = client
        .issues(
            &repo.owner,
            &repo.name,
            scope.state(),
            since.as_deref(),
            None,
        )
        .await
        .map_err(|error| error.to_string())?;

    let releases_count = releases.value.len();
    let issues_count = issues.value.len();
    let source = format!("github:{}/{}", repo.owner, repo.name);
    let mut entries = Vec::with_capacity(releases_count + issues_count);

    for release in &releases.value {
        entries.push((
            NewGithubEvent {
                repo_id: repo.id.clone(),
                source_event_id: format!("release:{}", release.id),
                event_type: GithubEventType::Release,
                payload_json: payload_without_body(release)?,
                occurred_at: release
                    .published_at
                    .clone()
                    .unwrap_or_else(|| release.created_at.clone()),
            },
            NewItem {
                kind: ItemKind::Github,
                source: source.clone(),
                url: Some(release.html_url.clone()),
                title: Some(format!(
                    "Release · {}",
                    release.name.as_deref().unwrap_or(&release.tag_name)
                )),
                summary: release.body.as_deref().and_then(excerpt),
                note: Some(format!("GitHub release · {}", release.tag_name)),
                body: release.body.clone(),
            },
        ));
    }

    for issue in &issues.value {
        let event_type = if issue.pull_request.is_some() {
            GithubEventType::PullRequest
        } else {
            GithubEventType::Issue
        };
        let kind_label = if event_type == GithubEventType::PullRequest {
            "PR"
        } else {
            "Issue"
        };
        entries.push((
            NewGithubEvent {
                repo_id: repo.id.clone(),
                source_event_id: format!("issue:{}:{}", issue.id, issue.updated_at),
                event_type,
                payload_json: payload_without_body(issue)?,
                occurred_at: issue.updated_at.clone(),
            },
            NewItem {
                kind: ItemKind::Github,
                source: source.clone(),
                url: Some(issue.html_url.clone()),
                title: Some(format!("{kind_label} · #{} {}", issue.number, issue.title)),
                summary: issue.body.as_deref().and_then(excerpt),
                note: Some(format!("GitHub {kind_label} · {}", issue.state)),
                body: issue.body.clone(),
            },
        ));
    }

    // 单仓库一次同步的所有写入共用一个事务。
    let imported_count = database
        .github_events()
        .insert_with_items(&entries)
        .map_err(|error| error.to_string())?;

    let synced_at = now_iso();
    let mut updated_repo = repo.clone();
    updated_repo.last_synced_at = Some(synced_at.clone());
    database
        .github_repos()
        .update(&updated_repo)
        .map_err(|error| error.to_string())?;

    Ok(SyncSummary {
        repo_id: repo.id.clone(),
        imported_count,
        releases_count,
        issues_count,
        // 没有新增条目即视为「内容没有变化」，不依赖 304。
        not_modified: imported_count == 0,
        synced_at,
    })
}

/// 事件载荷只留元信息：正文已经写进 `items.body`，不再重复存一份。
fn payload_without_body<T: serde::Serialize>(value: &T) -> Result<String, String> {
    let mut payload = serde_json::to_value(value).map_err(|error| error.to_string())?;
    if let Some(object) = payload.as_object_mut() {
        object.remove("body");
    }
    Ok(payload.to_string())
}

fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}
