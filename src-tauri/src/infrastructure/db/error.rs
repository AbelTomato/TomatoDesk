use std::{error::Error, fmt};

#[derive(Debug)]
pub enum DbError {
    Sqlite(rusqlite::Error),
    Io(std::io::Error),
    InvalidData(String),
    NotFound(&'static str),
    LockPoisoned,
}

impl fmt::Display for DbError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite(error) => write!(formatter, "database error: {error}"),
            Self::Io(error) => write!(formatter, "database filesystem error: {error}"),
            Self::InvalidData(message) => write!(formatter, "invalid database data: {message}"),
            Self::NotFound(entity) => write!(formatter, "{entity} not found"),
            Self::LockPoisoned => formatter.write_str("database connection lock poisoned"),
        }
    }
}

impl Error for DbError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Sqlite(error) => Some(error),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for DbError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

impl From<std::io::Error> for DbError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

pub type DbResult<T> = Result<T, DbError>;
