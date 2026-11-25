use serde::Serialize;
use tauri::State;

use crate::infrastructure::db::Database;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub environment: &'static str,
}

/// 本次启动执行了数据库迁移时，前端据此提示「旧 GitHub 条目已清理，需要重新同步」。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationNotice {
    pub migrated_to: Option<i64>,
    pub backup_path: Option<String>,
}

#[tauri::command]
pub fn get_app_info() -> AppInfo {
    AppInfo {
        name: "TomatoDesk",
        version: env!("CARGO_PKG_VERSION"),
        environment: if cfg!(debug_assertions) {
            "development"
        } else {
            "production"
        },
    }
}

#[tauri::command]
pub fn get_migration_notice(database: State<'_, Database>) -> MigrationNotice {
    MigrationNotice {
        migrated_to: database.migrated_to(),
        backup_path: database
            .migration_backup()
            .map(|path| path.to_string_lossy().into_owned()),
    }
}
