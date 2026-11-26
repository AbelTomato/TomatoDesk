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
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        platform::show_quick_capture_window(app);
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::get_migration_notice,
            commands::create_item,
            commands::get_item,
            commands::get_item_body,
            commands::list_items_page,
            commands::count_items_by_status,
            commands::update_item,
            commands::archive_item,
            commands::search_items,
            commands::capture_item,
            commands::open_capture,
            commands::dismiss_capture,
            commands::list_tags,
            commands::create_tag,
            commands::set_item_tags,
            commands::get_item_tags,
            commands::add_github_repo,
            commands::list_github_repos,
            commands::remove_github_repo,
            commands::get_github_token_status,
            commands::save_github_token,
            commands::delete_github_token,
            commands::sync_github_repo,
            commands::sync_all_github_repos,
            commands::list_github_sync_runs,
            commands::list_repos,
            commands::get_repo,
            commands::update_repo_meta,
            commands::list_repo_threads,
            commands::get_thread,
            commands::refresh_thread_comments,
            commands::list_repo_releases,
            commands::export_items_json,
            commands::export_items_markdown
        ])
        .setup(|app| {
            let database_path = app.path().app_data_dir()?.join("tomatodesk.sqlite3");
            let database = infrastructure::db::Database::open(database_path)?;
            app.manage(database.clone());
            application::github::start_scheduler(database, app.handle().clone());
            platform::setup(app)
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
