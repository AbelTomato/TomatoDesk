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
            commands::create_capture,
            commands::get_capture,
            commands::get_capture_body,
            commands::list_captures_page,
            commands::count_captures,
            commands::update_capture,
            commands::archive_capture,
            commands::restore_capture,
            commands::search_captures,
            commands::capture_from_clipboard,
            commands::finish_capture,
            commands::open_capture,
            commands::dismiss_capture,
            commands::list_tags,
            commands::create_tag,
            commands::set_capture_tags,
            commands::get_capture_tags,
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
            commands::create_task,
            commands::list_tasks_page,
            commands::count_tasks,
            commands::update_task,
            commands::complete_task,
            commands::cancel_task,
            commands::reopen_task,
            commands::list_inbox_messages_page,
            commands::count_inbox_messages,
            commands::list_inbox_sources,
            commands::list_email_accounts,
            commands::test_email_account,
            commands::save_email_account,
            commands::delete_email_account,
            commands::get_inbox_message,
            commands::apply_inbox_action,
            commands::undo_inbox_action,
            commands::mark_inbox_message_read,
            commands::mark_inbox_message_unread,
            commands::dismiss_inbox_message,
            commands::convert_inbox_message_to_task,
            commands::ingest_inbox_message,
            commands::get_inbox_read_back_status,
            commands::retry_inbox_read_back,
            commands::list_inbox_rules,
            commands::save_inbox_rule,
            commands::delete_inbox_rule,
            commands::preview_inbox_rule,
            commands::export_captures_json,
            commands::export_captures_markdown
        ])
        .setup(|app| {
            let database_path = app.path().app_data_dir()?.join("tomatodesk.sqlite3");
            let database = infrastructure::db::Database::open(database_path)?;
            app.manage(database.clone());
            application::github::start_scheduler(database.clone(), app.handle().clone());
            application::inbox::github::start_poller(database.clone(), app.handle().clone());
            application::inbox::email::start_poller(database.clone(), app.handle().clone());
            application::inbox::read_back::start_worker(database, app.handle().clone());
            platform::setup(app)
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
