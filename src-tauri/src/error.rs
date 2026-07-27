use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
    #[error(transparent)]
    Image(#[from] image::ImageError),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl AppError {
    pub fn msg(msg: impl Into<String>) -> Self {
        Self::Message(msg.into())
    }

    /// True when SQLite refused the lock (transient under concurrent writers).
    pub fn is_db_busy(&self) -> bool {
        match self {
            Self::Db(rusqlite::Error::SqliteFailure(e, _)) => matches!(
                e.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            ),
            other => {
                let msg = other.to_string().to_lowercase();
                msg.contains("database is locked") || msg.contains("database is busy")
            }
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;
