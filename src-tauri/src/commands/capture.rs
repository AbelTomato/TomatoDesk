use tauri::{AppHandle, State};

use crate::{application, domain::Item, infrastructure::db::Database, platform};

use super::{
    dto::{CaptureItemInput, parse_input},
    error::{CommandError, CommandResult},
};

#[tauri::command]
pub async fn capture_item(
    app: AppHandle,
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<Item> {
    let input = parse_input::<CaptureItemInput>(input)?.validate()?;
    let database = database.inner().clone();
    let item = application::capture::capture_clipboard_content(&database, input.value, input.note)
        .await
        .map_err(CommandError::from)?;

    platform::complete_quick_capture(&app);
    Ok(item)
}

#[tauri::command]
pub fn open_capture(app: AppHandle) {
    platform::show_quick_capture_window(&app);
}

#[tauri::command]
pub fn dismiss_capture(app: AppHandle) {
    platform::hide_quick_capture_window(&app);
}
