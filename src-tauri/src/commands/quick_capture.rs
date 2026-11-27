use tauri::{AppHandle, State};

use crate::{application, domain::Capture, infrastructure::db::Database, platform};

use super::{
    dto::{QuickCaptureInput, parse_input},
    error::{CommandError, CommandResult},
};

#[tauri::command]
pub fn capture_from_clipboard(
    app: AppHandle,
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<Capture> {
    let input = parse_input::<QuickCaptureInput>(input)?.validate()?;
    let captured =
        application::captures::capture_clipboard_content(&database, input.value, input.note)
            .map_err(CommandError::from)?;

    // 抓取放后台：命令只负责入库与派发，保存耗时不再取决于目标站点。
    if let Some(url) = captured.pending_link {
        application::captures::enrich_link_capture(
            database.inner().clone(),
            app,
            captured.capture.clone(),
            url,
        );
    }

    // 窗口收起交给前端的 finish_capture：保存成功后先让用户看到轻量反馈，
    // 再由前端决定何时收起，避免记录完立刻被带回主窗口。
    Ok(captured.capture)
}

#[tauri::command]
pub fn finish_capture(app: AppHandle) {
    platform::complete_quick_capture(&app);
}

#[tauri::command]
pub fn open_capture(app: AppHandle) {
    platform::show_quick_capture_window(&app);
}

#[tauri::command]
pub fn dismiss_capture(app: AppHandle) {
    platform::hide_quick_capture_window(&app);
}
