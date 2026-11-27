use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::{
    domain::{InboxSource, InboxSourceKind},
    infrastructure::db::{Database, DbError, DbResult},
};

use super::parse_value;

/// 开发用 ingest 需要一个稳定的来源；MVP 不暴露消息源管理界面，
/// 因此固定复用这一条记录，连接器接入后各自新建自己的来源。
pub const MANUAL_SOURCE_ID: &str = "source-manual";
const MANUAL_SOURCE_NAME: &str = "手动导入";

/// GitHub 通知连接器固定使用这一条来源，避免每次同步都新建来源。
pub const GITHUB_SOURCE_ID: &str = "source-github";
const GITHUB_SOURCE_NAME: &str = "GitHub 通知";

const SOURCE_FIELDS: &str =
    "id, kind, name, enabled, last_synced_at, last_error, created_at, updated_at";

type RawSource = (
    String,
    String,
    String,
    bool,
    Option<String>,
    Option<String>,
    String,
    String,
);

/// 邮箱来源一行：展示字段 + 非敏感配置原文（地址、主机、端口、加密方式、文件夹）。
///
/// 配置原文由 `infrastructure::email::EmailConfig` 负责解析，仓储层不解释 JSON。
#[derive(Debug, Clone)]
pub struct EmailSourceRow {
    pub source: InboxSource,
    pub config_json: String,
}

pub struct InboxSourceRepository {
    database: Database,
}

impl InboxSourceRepository {
    pub(crate) fn new(database: Database) -> Self {
        Self { database }
    }

    pub fn list(&self) -> DbResult<Vec<InboxSource>> {
        self.database.with_connection(|connection| {
            let mut statement = connection.prepare(&format!(
                "SELECT {SOURCE_FIELDS} FROM inbox_sources ORDER BY created_at ASC, id ASC"
            ))?;
            let rows = statement.query_map([], raw_source)?;
            rows.map(|row| decode_source(row?)).collect()
        })
    }

    pub fn get(&self, id: &str) -> DbResult<Option<InboxSource>> {
        self.database.with_connection(|connection| {
            let result = connection
                .query_row(
                    &format!("SELECT {SOURCE_FIELDS} FROM inbox_sources WHERE id = ?1"),
                    [id],
                    raw_source,
                )
                .optional()?;
            result.map(decode_source).transpose()
        })
    }

    /// 首次 ingest 时创建人工来源，之后直接复用；重复调用不会产生第二条。
    pub fn ensure_manual(&self) -> DbResult<InboxSource> {
        self.database.with_connection(|connection| {
            ensure_source(
                connection,
                MANUAL_SOURCE_ID,
                InboxSourceKind::Manual,
                MANUAL_SOURCE_NAME,
            )
        })
    }

    /// GitHub 连接器首次同步时创建来源，之后复用同一条。
    pub fn ensure_github(&self) -> DbResult<InboxSource> {
        self.database.with_connection(|connection| {
            ensure_source(
                connection,
                GITHUB_SOURCE_ID,
                InboxSourceKind::Github,
                GITHUB_SOURCE_NAME,
            )
        })
    }

    /// 保存邮箱账户：id 由「地址 + 主机」派生，所以同一账户重复保存是更新而不是新增。
    ///
    /// 冲突时清空 `cursor`：地址、主机、文件夹任一处改动后旧 UID 都可能失效，
    /// 按首次窗口重扫最安全；重复消息由 `(source_id, external_id)` 去重，不会产生副本。
    /// 同时清掉 `last_error`，因为用户刚刚可能已经改好了授权码。
    pub fn upsert_email(&self, id: &str, name: &str, config_json: &str) -> DbResult<InboxSource> {
        self.database.with_connection(|connection| {
            connection.execute(
                "INSERT INTO inbox_sources(id, kind, name, config_json)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(id) DO UPDATE SET
                     name = excluded.name,
                     config_json = excluded.config_json,
                     enabled = 1,
                     cursor = NULL,
                     last_error = NULL,
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
                params![id, InboxSourceKind::Email.as_str(), name, config_json],
            )?;
            connection
                .query_row(
                    &format!("SELECT {SOURCE_FIELDS} FROM inbox_sources WHERE id = ?1"),
                    [id],
                    raw_source,
                )
                .optional()?
                .map(decode_source)
                .transpose()?
                .ok_or(DbError::NotFound("inbox source"))
        })
    }

    /// 列出全部邮箱来源，连同非敏感配置原文一起返回，供同步与设置页使用。
    pub fn list_email(&self) -> DbResult<Vec<EmailSourceRow>> {
        self.database.with_connection(|connection| {
            let mut statement = connection.prepare(&format!(
                "SELECT {SOURCE_FIELDS}, config_json FROM inbox_sources
                 WHERE kind = ?1
                 ORDER BY created_at ASC, id ASC"
            ))?;
            let rows = statement.query_map([InboxSourceKind::Email.as_str()], |row| {
                Ok((raw_source(row)?, row.get::<_, String>(8)?))
            })?;
            rows.map(|row| {
                let (raw, config_json) = row?;
                Ok(EmailSourceRow {
                    source: decode_source(raw)?,
                    config_json,
                })
            })
            .collect()
        })
    }

    /// 非敏感配置原文；来源不存在时返回 `None`。
    pub fn config(&self, id: &str) -> DbResult<Option<String>> {
        self.database.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT config_json FROM inbox_sources WHERE id = ?1",
                    [id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(Into::into)
        })
    }

    pub fn save_config(&self, id: &str, config_json: &str) -> DbResult<()> {
        self.database.with_connection(|connection| {
            connection.execute(
                "UPDATE inbox_sources
                 SET config_json = ?2,
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1",
                params![id, config_json],
            )?;
            Ok(())
        })
    }

    /// 删除账户时清空配置。写 `{}` 而不是 `''` 或 `NULL`：
    /// 列上的 `CHECK (json_valid(config_json))` 会拒绝空串，`NULL` 又会让按 `String` 取列的
    /// `list_email` 报 `InvalidColumnType`；空对象两者都满足，且 `EmailConfig::parse` 判为未配置。
    pub fn clear_config(&self, id: &str) -> DbResult<()> {
        self.save_config(id, "{}")
    }

    /// 软停用：不删来源行，避免 `inbox_messages` 的 `ON DELETE CASCADE` 把历史消息一起清掉。
    pub fn set_enabled(&self, id: &str, enabled: bool) -> DbResult<()> {
        self.database.with_connection(|connection| {
            connection.execute(
                "UPDATE inbox_sources
                 SET enabled = ?2,
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1",
                params![id, enabled],
            )?;
            Ok(())
        })
    }

    /// 清游标用于「重新添加同一账户」与 UIDVALIDITY 变化后的全量重扫。
    pub fn clear_cursor(&self, id: &str) -> DbResult<()> {
        self.database.with_connection(|connection| {
            connection.execute(
                "UPDATE inbox_sources
                 SET cursor = NULL,
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1",
                [id],
            )?;
            Ok(())
        })
    }

    /// 上限校验只数启用中的邮箱账户：软停用的账户不占名额。
    pub fn count_configured_email(&self) -> DbResult<i64> {
        self.database.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT COUNT(*) FROM inbox_sources WHERE kind = ?1 AND enabled = 1",
                    [InboxSourceKind::Email.as_str()],
                    |row| row.get(0),
                )
                .map_err(Into::into)
        })
    }

    /// 同步游标不进 IPC，连接器单独读取。
    pub fn cursor(&self, id: &str) -> DbResult<Option<String>> {
        self.database.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT cursor FROM inbox_sources WHERE id = ?1",
                    [id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()
                .map(|value| value.flatten())
                .map_err(Into::into)
        })
    }

    /// 同步成功后更新游标与时间，并清掉上一次的错误。
    pub fn mark_synced(&self, id: &str, cursor: Option<&str>, synced_at: &str) -> DbResult<()> {
        self.database.with_connection(|connection| {
            connection.execute(
                "UPDATE inbox_sources
                 SET last_synced_at = ?2,
                     cursor = COALESCE(?3, cursor),
                     last_error = NULL,
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1",
                params![id, synced_at, cursor],
            )?;
            Ok(())
        })
    }

    /// 同步失败只记录错误，不动游标：下次仍从上次成功的位置继续。
    pub fn mark_failed(&self, id: &str, error: &str) -> DbResult<()> {
        self.database.with_connection(|connection| {
            connection.execute(
                "UPDATE inbox_sources
                 SET last_error = ?2,
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1",
                params![id, error],
            )?;
            Ok(())
        })
    }
}

fn ensure_source(
    connection: &Connection,
    id: &str,
    kind: InboxSourceKind,
    name: &str,
) -> DbResult<InboxSource> {
    connection.execute(
        "INSERT INTO inbox_sources(id, kind, name) VALUES (?1, ?2, ?3)
         ON CONFLICT(id) DO NOTHING",
        params![id, kind.as_str(), name],
    )?;
    connection
        .query_row(
            &format!("SELECT {SOURCE_FIELDS} FROM inbox_sources WHERE id = ?1"),
            [id],
            raw_source,
        )
        .optional()?
        .map(decode_source)
        .transpose()?
        .ok_or(DbError::NotFound("inbox source"))
}

fn raw_source(row: &Row<'_>) -> rusqlite::Result<RawSource> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
    ))
}

fn decode_source(row: RawSource) -> DbResult<InboxSource> {
    Ok(InboxSource {
        id: row.0,
        kind: parse_value(row.1, "inbox source kind")?,
        name: row.2,
        enabled: row.3,
        last_synced_at: row.4,
        last_error: row.5,
        created_at: row.6,
        updated_at: row.7,
    })
}
