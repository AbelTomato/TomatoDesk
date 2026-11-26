use tauri::{AppHandle, State};

use crate::{
    application,
    domain::{GithubRepo, SyncRun},
    infrastructure::{db::Database, github::parse_repository_url, secrets},
};

use super::{
    dto::{GithubRepoInput, GithubTokenInput, parse_input},
    error::{CommandError, CommandResult},
};

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubTokenStatus {
    pub configured: bool,
}

pub(super) fn add_github_repo_in(
    database: &Database,
    input: GithubRepoInput,
) -> CommandResult<GithubRepo> {
    let input = input.validate()?;
    let (owner, name) = parse_repository_url(&input.url)
        .map_err(|error| CommandError::validation(error.to_string()))?;
    database
        .github_repos()
        .create(&owner, &name)
        .map_err(Into::into)
}

#[tauri::command]
pub fn add_github_repo(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<GithubRepo> {
    add_github_repo_in(&database, parse_input(input)?)
}

#[tauri::command]
pub fn list_github_repos(database: State<'_, Database>) -> CommandResult<Vec<GithubRepo>> {
    database.github_repos().list().map_err(Into::into)
}

#[tauri::command]
pub fn remove_github_repo(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<()> {
    let input = parse_input::<super::dto::IdInput>(input)?.validate()?;
    if database
        .github_repos()
        .delete(&input.id)
        .map_err(CommandError::from)?
    {
        Ok(())
    } else {
        Err(CommandError::not_found("GitHub repository"))
    }
}

#[tauri::command]
pub fn get_github_token_status() -> CommandResult<GithubTokenStatus> {
    Ok(GithubTokenStatus {
        configured: secrets::get_github_token()
            .map_err(CommandError::internal)?
            .is_some(),
    })
}

#[tauri::command]
pub fn save_github_token(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<GithubTokenStatus> {
    let input = parse_input::<GithubTokenInput>(input)?.validate()?;
    secrets::save_github_token(&input.token).map_err(CommandError::internal)?;
    // 换 Token 往往就是为了修掉认证失败，顺手把 blocked 的已读回写放回队列。
    application::inbox::github::reset_after_token_saved(&database).map_err(CommandError::from)?;
    Ok(GithubTokenStatus { configured: true })
}

#[tauri::command]
pub fn delete_github_token(database: State<'_, Database>) -> CommandResult<GithubTokenStatus> {
    secrets::delete_github_token().map_err(CommandError::internal)?;
    // 没有凭据就写不出去，pending 行直接丢弃；blocked 行保留，继续提示需要重新授权。
    application::inbox::github::clear_pending_after_token_removed(&database)
        .map_err(CommandError::from)?;
    Ok(GithubTokenStatus { configured: false })
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SyncGithubRepoInput {
    id: String,
    scope: Option<application::github::SyncScope>,
}

impl SyncGithubRepoInput {
    fn validate(self) -> CommandResult<(String, application::github::SyncScope)> {
        let id = self.id.trim().to_owned();
        if id.is_empty() {
            return Err(CommandError::validation("id cannot be empty"));
        }
        Ok((
            id,
            self.scope
                .unwrap_or(application::github::SyncScope::Incremental),
        ))
    }
}

#[tauri::command]
pub async fn sync_github_repo(
    app: AppHandle,
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<application::github::SyncSummary> {
    let (id, scope) = parse_input::<SyncGithubRepoInput>(input)?.validate()?;
    let token = secrets::get_github_token()
        .map_err(CommandError::internal)?
        .ok_or_else(|| CommandError::validation("请先保存 GitHub Token"))?;
    let result = application::github::sync_github_repo(&database, &id, &token, scope).await;
    match &result {
        Ok(summary) => {
            let body = format!("同步完成，{} 条内容有更新。", summary.changed_count);
            let _ = crate::platform::show_notification(&app, "GitHub 同步完成", &body);
        }
        Err(error) => {
            let _ = crate::platform::show_notification(&app, "GitHub 同步失败", error);
        }
    }
    result.map_err(CommandError::internal)
}

#[tauri::command]
pub async fn sync_all_github_repos(
    app: AppHandle,
    database: State<'_, Database>,
) -> CommandResult<Vec<application::github::SyncSummary>> {
    let result = application::github::sync_all_github_repos(&database).await;
    match &result {
        Ok(summaries) => {
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
        Err(error) => {
            let _ = crate::platform::show_notification(&app, "GitHub 同步失败", error);
        }
    }
    result.map_err(CommandError::internal)
}

#[tauri::command]
pub fn list_github_sync_runs(database: State<'_, Database>) -> CommandResult<Vec<SyncRun>> {
    database
        .sync_runs()
        .list_latest("github", 20)
        .map_err(Into::into)
}
