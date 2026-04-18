//! Errors produced by the session layer.

use crate::AppError;

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// The caller-supplied session identifier is malformed (wrong length
    /// or bad charset). Treated as if the session simply did not exist —
    /// never hint at the reason to the client (OWASP ASVS 2.1.1).
    #[error("malformed session identifier")]
    Malformed,

    /// Session not found in Redis (never existed, destroyed, or expired).
    #[error("session not found")]
    NotFound,

    /// Session exists but its absolute lifetime is exhausted; the store
    /// should delete it and callers must treat this the same as
    /// `NotFound`.
    #[error("session absolute lifetime exceeded")]
    Expired,

    /// Session exists but the caller did not pass a matching CSRF token
    /// on an unsafe method.
    #[error("csrf token missing or invalid")]
    CsrfMismatch,

    /// Redis is unreachable, returned an error, or the cached payload
    /// failed to deserialise. Always a 500 for the caller.
    #[error(transparent)]
    Backend(#[from] anyhow::Error),
}

impl From<redis::RedisError> for SessionError {
    fn from(err: redis::RedisError) -> Self {
        Self::Backend(err.into())
    }
}

impl From<serde_json::Error> for SessionError {
    fn from(err: serde_json::Error) -> Self {
        Self::Backend(err.into())
    }
}

impl From<SessionError> for AppError {
    fn from(err: SessionError) -> Self {
        match err {
            SessionError::Malformed | SessionError::NotFound | SessionError::Expired => {
                AppError::Unauthorized
            }
            SessionError::CsrfMismatch => AppError::Forbidden,
            SessionError::Backend(err) => AppError::Internal(err),
        }
    }
}
