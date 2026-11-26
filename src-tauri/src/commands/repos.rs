use tauri::State;

use crate::{
    application,
    domain::{GithubRelease, RepoDetail, RepoSummary, ThreadDetail, ThreadPage},
    infrastructure::db::Database,
};

use super::{
    dto::{
        ItemIdInput, ListRepoThreadsInput, ThreadCommentsInput, UpdateRepoMetaInput, parse_input,
    },
    error::{CommandError, CommandResult},
};

pub(super) fn list_repos_in(database: &Database) -> CommandResult<Vec<RepoSummary>> {
    database.github_repos().list_summaries().map_err(Into::into)
}

pub(super) fn get_repo_in(database: &Database, input: ItemIdInput) -> CommandResult<RepoDetail> {
    let input = input.validate()?;
    database
        .github_repos()
        .detail(&input.id)
        .map_err(CommandError::from)?
        .ok_or_else(|| CommandError::not_found("GitHub repository"))
}

pub(super) fn update_repo_meta_in(
    database: &Database,
    input: UpdateRepoMetaInput,
) -> CommandResult<RepoDetail> {
    let (id, custom_description, custom_note) = input.validate()?;
    let changed = database
        .github_repos()
        .update_meta(&id, custom_description.as_deref(), custom_note.as_deref())
        .map_err(CommandError::from)?;
    if !changed {
        return Err(CommandError::not_found("GitHub repository"));
    }
    get_repo_in(database, ItemIdInput { id })
}

pub(super) fn list_repo_threads_in(
    database: &Database,
    input: ListRepoThreadsInput,
) -> CommandResult<ThreadPage> {
    let query = input.into_query()?;
    database.github_threads().page(&query).map_err(Into::into)
}

pub(super) fn get_thread_in(
    database: &Database,
    input: ItemIdInput,
) -> CommandResult<ThreadDetail> {
    let input = input.validate()?;
    database
        .github_threads()
        .get(&input.id)
        .map_err(CommandError::from)?
        .ok_or_else(|| CommandError::not_found("GitHub thread"))
}

pub(super) fn list_repo_releases_in(
    database: &Database,
    input: ItemIdInput,
) -> CommandResult<Vec<GithubRelease>> {
    let input = input.validate()?;
    database
        .github_releases()
        .list_for_repo(&input.id)
        .map_err(Into::into)
}

#[tauri::command]
pub fn list_repos(database: State<'_, Database>) -> CommandResult<Vec<RepoSummary>> {
    list_repos_in(&database)
}

#[tauri::command]
pub fn get_repo(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<RepoDetail> {
    get_repo_in(&database, parse_input(input)?)
}

#[tauri::command]
pub fn update_repo_meta(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<RepoDetail> {
    update_repo_meta_in(&database, parse_input(input)?)
}

#[tauri::command]
pub fn list_repo_threads(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<ThreadPage> {
    list_repo_threads_in(&database, parse_input(input)?)
}

#[tauri::command]
pub fn get_thread(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<ThreadDetail> {
    get_thread_in(&database, parse_input(input)?)
}

#[tauri::command]
pub async fn refresh_thread_comments(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<application::github::ThreadComments> {
    let (id, force) = parse_input::<ThreadCommentsInput>(input)?.validate()?;
    application::github::load_thread_comments(&database, &id, force)
        .await
        .map_err(CommandError::internal)
}

#[tauri::command]
pub fn list_repo_releases(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<Vec<GithubRelease>> {
    list_repo_releases_in(&database, parse_input(input)?)
}
