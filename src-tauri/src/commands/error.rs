use serde::Serialize;

use crate::infrastructure::{db::DbError, email::EmailError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandErrorCode {
    Validation,
    NotFound,
    Conflict,
    Internal,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: CommandErrorCode,
    pub message: String,
}

pub type CommandResult<T> = Result<T, CommandError>;

impl CommandError {
    pub(super) fn validation(message: impl Into<String>) -> Self {
        Self {
            code: CommandErrorCode::Validation,
            message: message.into(),
        }
    }

    pub(super) fn not_found(entity: &str) -> Self {
        Self {
            code: CommandErrorCode::NotFound,
            message: format!("{entity} not found"),
        }
    }

    pub(super) fn internal(message: impl Into<String>) -> Self {
        Self {
            code: CommandErrorCode::Internal,
            message: message.into(),
        }
    }
}

impl From<DbError> for CommandError {
    fn from(error: DbError) -> Self {
        match error {
            DbError::InvalidData(message) => Self::validation(message),
            DbError::NotFound(entity) => Self::not_found(entity),
            DbError::Sqlite(rusqlite::Error::SqliteFailure(error, _))
                if error.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY =>
            {
                Self::not_found("referenced record")
            }
            DbError::Sqlite(rusqlite::Error::SqliteFailure(error, _))
                if error.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                Self {
                    code: CommandErrorCode::Conflict,
                    message: "the requested data conflicts with an existing record".into(),
                }
            }
            DbError::Sqlite(_) | DbError::Io(_) | DbError::LockPoisoned => Self {
                code: CommandErrorCode::Internal,
                message: "database operation failed".into(),
            },
        }
    }
}

/// 邮件错误按「用户能自己修」分类：连不上、登录失败、文件夹不存在都是入参或账户状态问题；
/// 协议异常才归内部错误。文案直接复用 `EmailError` 的中文说明。
impl From<EmailError> for CommandError {
    fn from(error: EmailError) -> Self {
        let message = error.to_string();
        match error {
            EmailError::Unreachable(_) | EmailError::Auth(_) | EmailError::Mailbox(_) => {
                Self::validation(message)
            }
            EmailError::Protocol(_) | EmailError::Rejected(_) => Self::internal(message),
        }
    }
}
