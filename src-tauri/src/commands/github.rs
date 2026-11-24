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
    let input = parse_input::<super::dto::ItemIdInput>(input)?.validate()?;
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
pub fn save_github_token(input: Option<serde_json::Value>) -> CommandResult<GithubTokenStatus> {
    let input = parse_input::<GithubTokenInput>(input)?.validate()?;
    secrets::save_github_token(&input.token).map_err(CommandError::internal)?;
    Ok(GithubTokenStatus { configured: true })
}

#[tauri::command]
pub fn delete_github_token() -> CommandResult<GithubTokenStatus> {
    secrets::delete_github_token().map_err(CommandError::internal)?;
    Ok(GithubTokenStatus { configured: false })
}

#[tauri::command]
pub async fn sync_github_repo(
    app: AppHandle,
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<application::github::SyncSummary> {
    let input = parse_input::<super::dto::ItemIdInput>(input)?.validate()?;
    let token = secrets::get_github_token()
        .map_err(CommandError::internal)?
        .ok_or_else(|| CommandError::validation("请先保存 GitHub Token"))?;
    let result = application::github::sync_github_repo(&database, &input.id, &token).await;
    match &result {
        Ok(summary) => {
            let body = format!("同步完成，新增 {} 条更新。", summary.imported_count);
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
