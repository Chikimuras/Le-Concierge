//! Property-domain errors with `AppError` mapping.
//!
//! All unauthorised access / missing-resource cases collapse into
//! [`AppError::NotFound`] per ADR 0008 — the client cannot enumerate
//! which properties exist in another tenant's org.

use crate::{AppError, auth::AuthError};

#[derive(Debug, thiserror::Error)]
pub enum PropertyError {
    /// Slug or name failed domain validation.
    #[error("invalid property input: {0}")]
    Validation(&'static str),

    /// Slug collides with an existing (non-deleted) property in the
    /// same org. 409 — the caller owns the org, so revealing the
    /// conflict is safe.
    #[error("property slug already taken")]
    SlugAlreadyTaken,

    /// Property does not exist, or belongs to a different org, or has
    /// been soft-deleted. Always maps to 404 (never 403).
    #[error("property not found")]
    NotFound,

    #[error(transparent)]
    Repository(#[from] sqlx::Error),

    #[error(transparent)]
    Internal(anyhow::Error),
}

impl From<AuthError> for PropertyError {
    fn from(err: AuthError) -> Self {
        match err {
            // `Slug::parse` returns AuthError::InvalidSlug — bubble
            // through as a validation error rather than leaking a
            // different domain's error type.
            AuthError::InvalidSlug => Self::Validation("slug"),
            AuthError::Repository(e) => Self::Repository(e),
            other => Self::Internal(anyhow::anyhow!("unexpected auth error: {other}")),
        }
    }
}

impl From<PropertyError> for AppError {
    fn from(err: PropertyError) -> Self {
        match err {
            PropertyError::Validation(field) => {
                AppError::Validation(format!("champ invalide : {field}"))
            }
            PropertyError::SlugAlreadyTaken => {
                AppError::Conflict("cet identifiant de bien est déjà pris".into())
            }
            PropertyError::NotFound => AppError::NotFound,
            PropertyError::Repository(source) => AppError::Internal(source.into()),
            PropertyError::Internal(source) => AppError::Internal(source),
        }
    }
}
