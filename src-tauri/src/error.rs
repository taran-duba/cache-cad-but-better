use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Google Drive error {status}: {message}")]
    Drive { status: u16, message: String },
    #[error("network error: {0}")]
    Network(String),
    #[error("not signed in: {0}")]
    Auth(String),
    #[error("{name} is checked out by {by}")]
    Locked { name: String, by: String },
    #[error("{name} is used by {}", by.join(", "))]
    InUse { name: String, by: Vec<String> },
    #[error("your Google sign-in expired; sign in again")]
    SignInExpired,
    #[error("you don't have {0} checked out")]
    NotCheckedOut(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("check-in blocked: {0} problem(s) found")]
    Validation(usize),
    #[error("reference reader: {0}")]
    Refs(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("not configured: {0}")]
    Config(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl Error {
    /// Errors worth retrying with backoff (rate limits, server hiccups, network).
    pub fn is_transient(&self) -> bool {
        match self {
            Error::Network(_) => true,
            Error::Drive { status, message } => {
                *status == 429
                    || *status >= 500
                    || (*status == 403 && message.to_lowercase().contains("ratelimit"))
            }
            _ => false,
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::Network(e.to_string())
    }
}

// Tauri commands return errors to the UI as plain strings.
impl Serialize for Error {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
