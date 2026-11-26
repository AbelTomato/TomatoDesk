use tauri::{AppHandle, State};

use crate::{application, domain::Capture, infrastructure::db::Database, platform};

use super::{
    dto::{QuickCaptureInput, parse_input},
    error::{CommandError, CommandResult},
};

#[tauri::command]
pub async fn capture_from_clipboard(
    app: AppHandle,
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<Capture> {
    let input = parse_input::<QuickCaptureInput>(input)?.validate()?;
    let database = database.inner().clone();
    let capture =
        application::captures::capture_clipboard_content(&database, input.value, input.note)
            .await
            .map_err(CommandError::from)?;

    platform::complete_quick_capture(&app);
    Ok(capture)
}

#[tauri::command]
pub fn open_capture(app: AppHandle) {
    platform::show_quick_capture_window(&app);
}

#[tauri::command]
pub fn dismiss_capture(app: AppHandle) {
    platform::hide_quick_capture_window(&app);
}
