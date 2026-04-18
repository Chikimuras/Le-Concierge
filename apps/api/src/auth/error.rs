//! Auth-domain errors.
//!
//! [`AuthError`] converts cleanly into the app-wide [`crate::AppError`]
//! via `From`, so handlers can return either type through `?` without
//! ceremony. Error messages shown to end users remain French (CLAUDE.md
//! §9.8); the generic `AppError` mapping handles the localization.

use chrono::{DateTime, Utc};

use crate::AppError;

/// Errors produced by the `auth` module.
///
/// Public messages (via [`AppError`] conversion) never leak internal detail.
/// Source errors from `sqlx` / `argon2` are logged server-side and mapped
/// to a generic `Internal` variant.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// The supplied string is not a valid RFC 5321 email address.
    #[error("invalid email address")]
    InvalidEmail,

    /// Slug failed the regex check (lowercase kebab-case, 2..=64 chars).
    #[error("invalid organization slug")]
    InvalidSlug,

    /// Password violates the policy — for Phase 4a: length only, richer
    /// rules land with the signup endpoint in 4b.
    #[error("password does not meet the minimum policy")]
    WeakPassword,

    /// An active account already uses that email. Surfaces as a 409 so the
    /// frontend can suggest sign-in instead.
    #[error("email is already registered")]
    EmailAlreadyTaken,

    /// Slug collision on organization creation.
    #[error("organization slug already taken")]
    SlugAlreadyTaken,

    /// Credentials did not match. Deliberately opaque — never say whether
    /// the email exists or the password was wrong (OWASP ASVS 2.1.1).
    #[error("invalid credentials")]
    InvalidCredentials,

    /// Account is locked until `until` following too many failed attempts.
    /// Shown to the user only as a generic "try again later" (ASVS 2.2.1).
    #[error("account locked until {until}")]
    AccountLocked { until: DateTime<Utc> },

    /// Argon2id itself failed (e.g. the pepper is misconfigured, or the
    /// stored hash is malformed). Treat as 500.
    #[error("password hashing error")]
    Hashing(#[source] argon2::password_hash::Error),

    /// Hashing primitive could not be configured (params out of bounds,
    /// pepper too long, ...). Programmer or operator error — a user cannot
    /// fix this. Always maps to 500.
    #[error("hashing misconfigured: {0}")]
    HashingConfig(&'static str),

    /// `APP_AUTH__TOTP_KEY` missing, not 64 hex chars, or not 32 bytes
    /// after decoding. Operator config error — the api fails closed at
    /// boot. Maps to 500.
    #[error("TOTP encryption key missing or malformed")]
    TotpKeyInvalid,

    /// AES-GCM wrap or unwrap failed — either a tampered ciphertext or
    /// a missing/unexpected internal state. Always a 500 with a generic
    /// message (no oracle on which byte flipped).
    #[error("TOTP secret cryptography failure")]
    TotpCrypto,

    /// Caller already has an active 2FA enrollment and tried to start
    /// another without disabling the first. 409 Conflict.
    #[error("2FA already enrolled")]
    TotpAlreadyEnrolled,

    /// Caller submitted a verify / disable request on a user that has
    /// no 2FA row. 400 Validation.
    #[error("2FA not enrolled")]
    TotpNotEnrolled,

    /// Submitted TOTP code (or recovery code) did not match. Mapped to
    /// a 401 same as [`AuthError::InvalidCredentials`] so the client
    /// cannot tell the two apart from the outside.
    #[error("invalid 2FA code")]
    TotpInvalidCode,

    /// Any SQLx error. Inner detail logged; client sees 500.
    #[error(transparent)]
    Repository(#[from] sqlx::Error),

    /// Catch-all for unexpected failures bubbling up from an adjacent
    /// module (session store, external HTTP call, …). Always a 500.
    /// Carries an `anyhow::Error` so the source chain is preserved in
    /// logs without coupling this enum to every possible source type.
    #[error(transparent)]
    Internal(anyhow::Error),
}

impl From<AuthError> for AppError {
    fn from(err: AuthError) -> Self {
        match err {
            AuthError::InvalidEmail | AuthError::InvalidSlug | AuthError::WeakPassword => {
                AppError::Validation(err.to_string())
            }

            AuthError::EmailAlreadyTaken => AppError::Conflict("cet email est déjà utilisé".into()),
            AuthError::SlugAlreadyTaken => {
                AppError::Conflict("ce slug d'organisation est déjà pris".into())
            }

            AuthError::InvalidCredentials => AppError::Unauthorized,
            AuthError::AccountLocked { .. } => {
                // Do not reveal the lockout window — preserves the opacity
                // requirement of OWASP ASVS 2.1.1.
                AppError::Unauthorized
            }

            AuthError::Hashing(source) => {
                tracing::error!(error = %source, "argon2 failure");
                AppError::Internal(anyhow::anyhow!("hashing failure"))
            }
            AuthError::HashingConfig(detail) => {
                tracing::error!(detail = detail, "argon2 misconfiguration");
                AppError::Internal(anyhow::anyhow!("hashing misconfigured: {detail}"))
            }

            AuthError::TotpKeyInvalid => {
                tracing::error!("TOTP encryption key missing or malformed");
                AppError::Internal(anyhow::anyhow!("totp key misconfigured"))
            }
            AuthError::TotpCrypto => {
                tracing::error!("TOTP crypto failure (tampered ciphertext or bad key?)");
                AppError::Internal(anyhow::anyhow!("totp crypto failure"))
            }
            AuthError::TotpAlreadyEnrolled => AppError::Conflict("2FA déjà activé".into()),
            AuthError::TotpNotEnrolled => {
                AppError::Validation("2FA non configuré pour ce compte".into())
            }
            // Same shape as InvalidCredentials — the client cannot tell
            // "wrong code" apart from "not your account" (ASVS 2.1.1).
            AuthError::TotpInvalidCode => AppError::Unauthorized,

            AuthError::Repository(source) => AppError::Internal(source.into()),
            AuthError::Internal(source) => AppError::Internal(source),
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use axum::http::StatusCode;

    use super::*;

    #[test]
    fn invalid_credentials_always_401_and_opaque() {
        let app_err: AppError = AuthError::InvalidCredentials.into();
        assert_eq!(app_err.status(), StatusCode::UNAUTHORIZED);
        assert!(!app_err.public_message().contains("introuvable"));
    }

    #[test]
    fn account_locked_does_not_leak_until_in_public_message() {
        let err = AuthError::AccountLocked {
            until: "2030-01-01T00:00:00Z".parse().expect("valid datetime"),
        };
        let app_err: AppError = err.into();
        let msg = app_err.public_message();
        assert!(!msg.contains("2030"));
    }

    #[test]
    fn email_already_taken_is_409() {
        let app_err: AppError = AuthError::EmailAlreadyTaken.into();
        assert_eq!(app_err.status(), StatusCode::CONFLICT);
    }

    #[test]
    fn invalid_email_is_422() {
        let app_err: AppError = AuthError::InvalidEmail.into();
        assert_eq!(app_err.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}
