//! 检查更新的周期触发器：只在「到点」时提醒前端，网络请求与界面展示仍归前端。
//!
//! 主窗口隐藏后 WebView 的定时器会被节流甚至挂起，把节奏放在 Rust 侧才靠得住。

use std::time::Duration;

use tauri::{AppHandle, Emitter};

/// 启动后先等一会儿：首屏加载、首轮同步都要用网络，别在这时候插一脚。
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(60);
/// 后台常驻进程的节奏：4 小时一次足够及时，也不会给 GitHub 添麻烦。
const CHECK_INTERVAL: Duration = Duration::from_secs(4 * 60 * 60);

pub(crate) const CHECK_DUE_EVENT: &str = "update-check-due";

pub fn start_checker(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK_DELAY).await;
        loop {
            let _ = app.emit(CHECK_DUE_EVENT, ());
            tokio::time::sleep(CHECK_INTERVAL).await;
        }
    });
}
