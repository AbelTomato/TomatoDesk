use tauri::AppHandle;

use crate::platform;

use super::error::{CommandError, CommandResult};

/// 主窗口不可见时的更新提示：系统通知是唯一的可见通道。
/// 打开主窗口后由右下角卡片或小胶囊接手，通知本身不做点击跳转。
#[tauri::command]
pub fn notify_update_available(app: AppHandle, version: String) -> CommandResult<()> {
    let version = version.trim();
    if version.is_empty() {
        return Err(CommandError::validation("version is required"));
    }
    platform::show_notification(
        &app,
        "TomatoDesk 有新版本",
        &format!("v{version} 已发布，打开 TomatoDesk 即可更新。"),
    )
    .map_err(|error| CommandError::internal(error.to_string()))
}
