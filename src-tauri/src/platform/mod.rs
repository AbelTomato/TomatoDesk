use std::error::Error;

use tauri::{
    App, AppHandle, Emitter, Manager, Runtime, WindowEvent,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};
use tauri_plugin_notification::NotificationExt;

const MAIN_WINDOW: &str = "main";
const QUICK_CAPTURE_WINDOW: &str = "quick-capture";

pub fn setup<R: Runtime>(app: &mut App<R>) -> Result<(), Box<dyn Error>> {
    // 全局快捷键属于操作系统级命名空间，不随 identifier 隔离，被占用是常态：
    // 注册失败只记一笔，托盘与界面照常可用，不能因此让整个应用启动失败。
    if let Err(error) = app.handle().global_shortcut().register(capture_shortcut()) {
        eprintln!("全局快捷键注册失败，托盘与界面仍可用：{error}");
    }

    install_tray(app)?;
    hide_windows_on_close(app);
    Ok(())
}

/// 生产用 `Ctrl + Shift + Space`；debug 构建再加上 `Alt`，
/// 这样同一个桌面既跑生产版又跑开发版时两个热键各自生效。
fn capture_shortcut() -> Shortcut {
    #[cfg(debug_assertions)]
    let modifiers = Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT;
    #[cfg(not(debug_assertions))]
    let modifiers = Modifiers::CONTROL | Modifiers::SHIFT;
    Shortcut::new(Some(modifiers), Code::Space)
}

pub fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    show_window(app, MAIN_WINDOW);
}

pub fn show_quick_capture_window<R: Runtime>(app: &AppHandle<R>) {
    show_window(app, QUICK_CAPTURE_WINDOW);
    if let Some(window) = app.get_webview_window(QUICK_CAPTURE_WINDOW) {
        let _ = window.emit("quick-capture-opened", ());
    }
}

/// 保存完成后只收起随手记窗口，不把主窗口带到前台。
/// 用户通常是在别的应用里按快捷键记录，主窗口是否已经打开由用户自己决定。
pub fn complete_quick_capture<R: Runtime>(app: &AppHandle<R>) {
    hide_quick_capture_window(app);
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        // 主窗口隐藏时事件也会被收到，下次打开时数据和角标已经是最新的。
        let _ = window.emit("quick-capture-completed", ());
    }
}

pub fn hide_quick_capture_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(QUICK_CAPTURE_WINDOW) {
        let _ = window.hide();
    }
}

pub fn show_notification<R: Runtime>(
    app: &AppHandle<R>,
    title: &str,
    body: &str,
) -> Result<(), Box<dyn Error>> {
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()?;
    Ok(())
}

fn show_window<R: Runtime>(app: &AppHandle<R>, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn install_tray<R: Runtime>(app: &App<R>) -> tauri::Result<()> {
    let show_main = MenuItem::with_id(app, "show-main", "显示主窗口", true, None::<&str>)?;
    let quick_capture = MenuItem::with_id(app, "quick-capture", "快速收集", true, None::<&str>)?;
    let check_updates = MenuItem::with_id(app, "check-updates", "检查更新", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "退出 TomatoDesk", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &show_main,
            &quick_capture,
            &check_updates,
            &separator,
            &quit,
        ],
    )?;

    let mut tray = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("TomatoDesk")
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show-main" => show_main_window(app),
            "quick-capture" => show_quick_capture_window(app),
            "check-updates" => {
                show_main_window(app);
                if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                    let _ = window.emit("check-updates-requested", ());
                }
            }
            "quit" => app.exit(0),
            _ => {}
        });

    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }

    tray.build(app)?;
    Ok(())
}

fn hide_windows_on_close<R: Runtime>(app: &App<R>) {
    for label in [MAIN_WINDOW, QUICK_CAPTURE_WINDOW] {
        if let Some(window) = app.get_webview_window(label) {
            let window_to_hide = window.clone();
            window.on_window_event(move |event| {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window_to_hide.hide();
                }
            });
        }
    }
}
