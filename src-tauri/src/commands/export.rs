use std::{fs, path::PathBuf};

use chrono::Utc;
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::{application, infrastructure::db::Database};

use super::error::{CommandError, CommandResult};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub path: String,
    pub capture_count: usize,
}

#[tauri::command]
pub async fn export_captures_json(
    app: AppHandle,
    database: State<'_, Database>,
) -> CommandResult<Option<ExportResult>> {
    let bundle =
        application::export::build_export_bundle(&database).map_err(CommandError::internal)?;
    let capture_count = bundle.captures.len();
    let content = application::export::to_json(&bundle).map_err(CommandError::internal)?;
    save_export(
        &app,
        &format!("TomatoDesk-export-{}.json", Utc::now().format("%Y-%m-%d")),
        "JSON 文件",
        "json",
        content,
        capture_count,
    )
}

#[tauri::command]
pub async fn export_captures_markdown(
    app: AppHandle,
    database: State<'_, Database>,
) -> CommandResult<Option<ExportResult>> {
    let bundle =
        application::export::build_export_bundle(&database).map_err(CommandError::internal)?;
    let capture_count = bundle.captures.len();
    let content = application::export::to_markdown(&bundle);
    save_export(
        &app,
        &format!("TomatoDesk-export-{}.md", Utc::now().format("%Y-%m-%d")),
        "Markdown 文件",
        "md",
        content,
        capture_count,
    )
}

fn save_export(
    app: &AppHandle,
    file_name: &str,
    filter_name: &str,
    extension: &str,
    content: String,
    capture_count: usize,
) -> CommandResult<Option<ExportResult>> {
    let selected_path = app
        .dialog()
        .file()
        .set_title("导出 TomatoDesk 数据")
        .set_file_name(file_name)
        .add_filter(filter_name, &[extension])
        .blocking_save_file();
    let Some(selected_path) = selected_path else {
        return Ok(None);
    };
    let path: PathBuf = selected_path
        .into_path()
        .map_err(|_| CommandError::internal("无法解析导出文件路径"))?;
    fs::write(&path, content.as_bytes())
        .map_err(|error| CommandError::internal(format!("写入导出文件失败：{error}")))?;

    Ok(Some(ExportResult {
        path: path.to_string_lossy().into_owned(),
        capture_count,
    }))
}
