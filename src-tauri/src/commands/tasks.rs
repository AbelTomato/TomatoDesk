use tauri::State;

use crate::{
    application,
    domain::{Task, TaskCounts, TaskPage, TaskStatus},
    infrastructure::db::Database,
};

use super::{
    dto::{CreateTaskInput, IdInput, ListTasksPageInput, UpdateTaskInput, parse_input},
    error::{CommandError, CommandResult},
};

pub(super) fn create_task_in(database: &Database, input: CreateTaskInput) -> CommandResult<Task> {
    let (title, note) = input.validate()?;
    application::tasks::create_task(database, title, note).map_err(Into::into)
}

pub(super) fn list_tasks_page_in(
    database: &Database,
    input: ListTasksPageInput,
) -> CommandResult<TaskPage> {
    let query = input.into_query()?;
    database.tasks().page(&query).map_err(Into::into)
}

pub(super) fn count_tasks_in(database: &Database) -> CommandResult<TaskCounts> {
    database.tasks().counts().map_err(Into::into)
}

pub(super) fn update_task_in(database: &Database, input: UpdateTaskInput) -> CommandResult<Task> {
    let (id, title, note) = input.validate()?;
    application::tasks::update_task(database, &id, title, note)
        .map_err(CommandError::from)?
        .ok_or_else(|| CommandError::not_found("task"))
}

fn change_status_in(
    database: &Database,
    input: IdInput,
    status: TaskStatus,
) -> CommandResult<Task> {
    let input = input.validate()?;
    application::tasks::set_status(database, &input.id, status)
        .map_err(CommandError::from)?
        .ok_or_else(|| CommandError::not_found("task"))
}

pub(super) fn complete_task_in(database: &Database, input: IdInput) -> CommandResult<Task> {
    change_status_in(database, input, TaskStatus::Done)
}

pub(super) fn cancel_task_in(database: &Database, input: IdInput) -> CommandResult<Task> {
    change_status_in(database, input, TaskStatus::Cancelled)
}

pub(super) fn reopen_task_in(database: &Database, input: IdInput) -> CommandResult<Task> {
    change_status_in(database, input, TaskStatus::Open)
}

#[tauri::command]
pub fn create_task(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<Task> {
    create_task_in(&database, parse_input(input)?)
}

#[tauri::command]
pub fn list_tasks_page(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<TaskPage> {
    list_tasks_page_in(&database, parse_input(input)?)
}

#[tauri::command]
pub fn count_tasks(database: State<'_, Database>) -> CommandResult<TaskCounts> {
    count_tasks_in(&database)
}

#[tauri::command]
pub fn update_task(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<Task> {
    update_task_in(&database, parse_input(input)?)
}

#[tauri::command]
pub fn complete_task(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<Task> {
    complete_task_in(&database, parse_input(input)?)
}

#[tauri::command]
pub fn cancel_task(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<Task> {
    cancel_task_in(&database, parse_input(input)?)
}

#[tauri::command]
pub fn reopen_task(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<Task> {
    reopen_task_in(&database, parse_input(input)?)
}
