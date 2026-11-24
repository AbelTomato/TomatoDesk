mod application;
mod commands;
mod domain;
mod infrastructure;
mod platform;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        platform::show_quick_capture_window(app);
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![commands::get_app_info])
        .setup(platform::setup)
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
