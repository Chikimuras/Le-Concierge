//! Invite-domain errors. ADR 0009 → 404-over-403 enumeration rule:
//! unknown token, cancelled invite, email mismatch, and "caller not a
//! manager" all collapse into `AppError::NotFound`. Expired is the
//! one exception — we surface 410 with `kind = invite_expired` so the
//! frontend can tell the user "ask for a new invite".

use crate::{AppError, auth::AuthError, email::EmailError};

#[derive(Debug, thiserror::Error)]
pub enum InviteError {
    /// Any of: unknown token, cancelled invite, email mismatch on
    /// accept, caller not a manager. Always 404.
    #[error("invite not found")]
    NotFound,

    /// Token matched but the pending row is past its TTL. 410 Gone.
    #[error("invite expired")]
    Expired,

    /// A pending invite already exists for this (org, email). 409
    /// — the manager just resends the existing one instead.
    #[error("pending invite already exists")]
    AlreadyPending,

    /// Accept-as-new-user tried to register with an email that already
    /// has a user row. 409 — user should login first then accept.
    #[error("email already registered")]
    EmailAlreadyTaken,

    /// The caller passed an invalid email / role when creating an
    /// invite. 422.
    #[error("invalid input: {0}")]
    Validation(&'static str),

    #[error(transparent)]
    Auth(#[from] AuthError),

    #[error(transparent)]
    Email(#[from] EmailError),

    #[error(transparent)]
    Repository(#[from] sqlx::Error),

    #[error(transparent)]
    Internal(anyhow::Error),
}

impl From<InviteError> for AppError {
    fn from(err: InviteError) -> Self {
        match err {
            InviteError::NotFound => AppError::NotFound,
            InviteError::Expired => AppError::Gone("invitation expirée".into()),
            InviteError::AlreadyPending => AppError::Conflict(
                "une invitation est déjà en attente pour cet email dans cette organisation".into(),
            ),
            InviteError::EmailAlreadyTaken => {
                AppError::Conflict("cet email est déjà utilisé par un compte existant".into())
            }
            InviteError::Validation(detail) => {
                AppError::Validation(format!("champ invalide : {detail}"))
            }
            InviteError::Auth(inner) => inner.into(),
            InviteError::Email(inner) => {
                tracing::error!(error = %inner, "invite email delivery failed");
                AppError::Internal(anyhow::anyhow!("email delivery failed"))
            }
            InviteError::Repository(source) => AppError::Internal(source.into()),
            InviteError::Internal(source) => AppError::Internal(source),
        }
    }
}
