mod application;
mod commands;
pub mod domain;
pub mod infrastructure;
mod platform;

use tauri::Manager;

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
        .setup(|app| {
            let database_path = app.path().app_data_dir()?.join("tomatodesk.sqlite3");
            app.manage(infrastructure::db::Database::open(database_path)?);
            platform::setup(app)
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
