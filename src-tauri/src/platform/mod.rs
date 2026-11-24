use std::error::Error;

use tauri::{
    App, AppHandle, Manager, Runtime, WindowEvent,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Modifiers, Shortcut};
use tauri_plugin_notification::NotificationExt;

const MAIN_WINDOW: &str = "main";
const QUICK_CAPTURE_WINDOW: &str = "quick-capture";

pub fn setup<R: Runtime>(app: &mut App<R>) -> Result<(), Box<dyn Error>> {
    app.handle().global_shortcut().register(Shortcut::new(
        Some(Modifiers::CONTROL | Modifiers::SHIFT),
        tauri_plugin_global_shortcut::Code::Space,
    ))?;

    install_tray(app)?;
    hide_windows_on_close(app);
    Ok(())
}

pub fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    show_window(app, MAIN_WINDOW);
}

pub fn show_quick_capture_window<R: Runtime>(app: &AppHandle<R>) {
    show_window(app, QUICK_CAPTURE_WINDOW);
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
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "退出 TomatoDesk", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_main, &quick_capture, &separator, &quit])?;

    let mut tray = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("TomatoDesk")
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show-main" => show_main_window(app),
            "quick-capture" => show_quick_capture_window(app),
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
