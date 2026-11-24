use std::time::Duration;

use chrono::{SecondsFormat, Utc};
use tauri::Emitter;

use crate::{
    domain::{GithubEventType, NewGithubEvent, NewItem, NewSyncRun, SyncRunStatus},
    infrastructure::{db::Database, github::GithubClient, secrets},
};

const SYNC_SOURCE: &str = "github";

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct EndpointEtags {
    releases: Option<String>,
    issues: Option<String>,
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

    let result = sync_repo_contents(database, &repo, token).await;
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
        match sync_github_repo(database, &repo.id, &token).await {
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
) -> Result<SyncSummary, String> {
    let client = GithubClient::new(token.to_owned(), Duration::from_secs(20))
        .map_err(|error| error.to_string())?;
    let endpoint_etags = repo
        .etag
        .as_deref()
        .and_then(|value| serde_json::from_str::<EndpointEtags>(value).ok())
        .unwrap_or(EndpointEtags {
            releases: None,
            issues: None,
        });
    let releases = client
        .releases(&repo.owner, &repo.name, endpoint_etags.releases.as_deref())
        .await
        .map_err(|error| error.to_string())?;
    let releases_etag = releases.etag.clone();
    let releases_not_modified = releases.not_modified;
    let releases_count = releases.value.len();
    let issues = client
        .issues(&repo.owner, &repo.name, endpoint_etags.issues.as_deref())
        .await
        .map_err(|error| error.to_string())?;
    let issues_etag = issues.etag.clone();
    let issues_not_modified = issues.not_modified;
    let issues_count = issues.value.len();
    let source = format!("github:{}/{}", repo.owner, repo.name);
    let mut imported_count = 0;

    for release in &releases.value {
        let inserted = database
            .github_events()
            .insert_with_item(
                &NewGithubEvent {
                    repo_id: repo.id.clone(),
                    source_event_id: format!("release:{}", release.id),
                    event_type: GithubEventType::Release,
                    payload_json: serde_json::to_string(release)
                        .map_err(|error| error.to_string())?,
                    occurred_at: release
                        .published_at
                        .clone()
                        .unwrap_or_else(|| release.created_at.clone()),
                },
                NewItem {
                    kind: crate::domain::ItemKind::Github,
                    source: source.clone(),
                    url: Some(release.html_url.clone()),
                    title: Some(format!(
                        "Release · {}",
                        release.name.as_deref().unwrap_or(&release.tag_name)
                    )),
                    summary: release.body.clone(),
                    note: Some(format!("GitHub release · {}", release.tag_name)),
                },
            )
            .map_err(|error| error.to_string())?;
        if inserted {
            imported_count += 1;
        }
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
        let inserted = database
            .github_events()
            .insert_with_item(
                &NewGithubEvent {
                    repo_id: repo.id.clone(),
                    source_event_id: format!("issue:{}:{}", issue.id, issue.updated_at),
                    event_type,
                    payload_json: serde_json::to_string(issue)
                        .map_err(|error| error.to_string())?,
                    occurred_at: issue.updated_at.clone(),
                },
                NewItem {
                    kind: crate::domain::ItemKind::Github,
                    source: source.clone(),
                    url: Some(issue.html_url.clone()),
                    title: Some(format!("{kind_label} · #{} {}", issue.number, issue.title)),
                    summary: issue.body.clone(),
                    note: Some(format!("GitHub {kind_label} · {}", issue.state)),
                },
            )
            .map_err(|error| error.to_string())?;
        if inserted {
            imported_count += 1;
        }
    }

    let synced_at = now_iso();
    let mut updated_repo = repo.clone();
    updated_repo.last_synced_at = Some(synced_at.clone());
    updated_repo.etag = Some(
        serde_json::to_string(&EndpointEtags {
            releases: releases_etag.or(endpoint_etags.releases),
            issues: issues_etag.or(endpoint_etags.issues),
        })
        .map_err(|error| error.to_string())?,
    );
    database
        .github_repos()
        .update(&updated_repo)
        .map_err(|error| error.to_string())?;

    Ok(SyncSummary {
        repo_id: repo.id.clone(),
        imported_count,
        releases_count,
        issues_count,
        not_modified: releases_not_modified && issues_not_modified,
        synced_at,
    })
}

fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}
