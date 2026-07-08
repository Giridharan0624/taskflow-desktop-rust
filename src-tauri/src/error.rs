use serde::{Serialize, Serializer};

/// Application error type returned from Tauri commands.
///
/// Serializes to `{ "code": "...", "message": "..." }` so the frontend can
/// switch on a stable `code` (e.g. detect `not_authenticated` / `unauthorized`
/// the way the Go frontend keys off the `errNotAuthenticated` sentinel).
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// No active session — the post-auth command guard rejected the call.
    #[error("not authenticated")]
    NotAuthenticated,

    /// A backend/API call returned 401. Distinct from `NotAuthenticated` so the
    /// UI can trigger a re-login flow / `auth:expired` event.
    #[error("unauthorized")]
    Unauthorized,

    /// Transport-level failure (DNS/TLS/timeout). Distinct so callers can fall
    /// back to on-disk caches rather than surfacing a hard error.
    #[error("{0}")]
    Network(String),

    /// Any other error, surfaced with a human-readable message.
    #[error("{0}")]
    Message(String),
}

impl AppError {
    fn code(&self) -> &'static str {
        match self {
            AppError::NotAuthenticated => "not_authenticated",
            AppError::Unauthorized => "unauthorized",
            AppError::Network(_) => "network",
            AppError::Message(_) => "error",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("code", self.code())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;

impl From<String> for AppError {
    fn from(s: String) -> Self {
        AppError::Message(s)
    }
}

impl From<&str> for AppError {
    fn from(s: &str) -> Self {
        AppError::Message(s.to_string())
    }
}
