//! Centralized error type and HTTP mapping.
//!
//! Every fallible handler returns [`Result<T, AppError>`]. [`AppError`]
//! implements [`IntoResponse`] so handlers can return it directly, and it
//! renders as a [RFC 7807] `application/problem+json` body. Internal detail
//! (source errors, stack context) is logged server-side and **never** leaked
//! to the client, satisfying OWASP ASVS 7.4.1.
//!
//! Per `CLAUDE.md` §7.1, every library module uses `thiserror`; only
//! `main.rs` / bootstrap may use `anyhow` at the edges.
//!
//! [RFC 7807]: https://www.rfc-editor.org/rfc/rfc7807

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

/// Application-wide error type.
///
/// New variants must map to a single HTTP status code. When in doubt prefer
/// a more specific variant over [`AppError::Internal`], which always 500s.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// The requested resource does not exist (or the caller is not allowed to
    /// know it exists — 404 is preferred over 403 to avoid disclosure).
    #[error("resource not found")]
    NotFound,

    /// No valid authentication credentials were provided.
    #[error("authentication required")]
    Unauthorized,

    /// Authenticated caller is not permitted to perform this action.
    #[error("forbidden")]
    Forbidden,

    /// Input failed validation. Carries a human-readable reason that is safe
    /// to show to end users (already in French per CLAUDE.md §9.8).
    #[error("validation failed: {0}")]
    Validation(String),

    /// The request conflicts with current server state (e.g. duplicate key).
    #[error("conflict: {0}")]
    Conflict(String),

    /// Caller is being rate-limited. Produced by `tower-governor`, wrapped
    /// here so upstream code can treat it uniformly.
    #[error("too many requests")]
    RateLimited,

    /// A required dependency is temporarily unavailable.
    #[error("service unavailable")]
    Unavailable,

    /// Catch-all for unexpected failures. Source error is kept for logging
    /// but the client only sees a generic 500 message.
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl AppError {
    /// HTTP status code this error maps to.
    #[must_use]
    pub fn status(&self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Short, stable identifier usable by clients for localized UI mapping.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::Unauthorized => "unauthorized",
            Self::Forbidden => "forbidden",
            Self::Validation(_) => "validation",
            Self::Conflict(_) => "conflict",
            Self::RateLimited => "rate_limited",
            Self::Unavailable => "unavailable",
            Self::Internal(_) => "internal",
        }
    }

    /// French-language message safe to expose to end users. Matches the
    /// default locale required by `CLAUDE.md` §9.8. Frontend may replace
    /// this via i18n based on `kind`.
    #[must_use]
    pub fn public_message(&self) -> String {
        match self {
            Self::NotFound => "Ressource introuvable.".into(),
            Self::Unauthorized => "Authentification requise.".into(),
            Self::Forbidden => "Action non autorisée.".into(),
            Self::Validation(reason) => format!("Données invalides : {reason}"),
            Self::Conflict(reason) => format!("Conflit : {reason}"),
            Self::RateLimited => "Trop de requêtes, réessayez plus tard.".into(),
            Self::Unavailable => "Service momentanément indisponible.".into(),
            Self::Internal(_) => "Erreur interne du serveur.".into(),
        }
    }
}

/// RFC 7807 problem document.
///
/// Rendered as `application/problem+json`. Fields beyond RFC 7807:
///
/// - `kind`: stable identifier for client-side mapping.
/// - `trace_id`: W3C traceparent id if one was propagated, to ease support.
#[derive(Debug, Serialize)]
struct ProblemDetails {
    #[serde(rename = "type")]
    type_: &'static str,
    title: &'static str,
    status: u16,
    detail: String,
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    trace_id: Option<String>,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        // Server-side logging. `Internal` carries a source chain we want to
        // see with full context; other variants are expected enough that
        // `debug` level is enough to avoid log noise.
        match &self {
            Self::Internal(err) => {
                tracing::error!(error = %err, error_debug = ?err, "internal server error");
            }
            other => {
                tracing::debug!(error = %other, "request failed with {}", other.kind());
            }
        }

        let status = self.status();
        // Real W3C traceparent extraction lands with the OpenTelemetry wiring
        // in Phase 5; until then the field is always omitted (it is
        // `skip_serializing_if = "Option::is_none"`).
        let trace_id: Option<String> = None;

        let body = ProblemDetails {
            type_: "about:blank",
            title: reason_phrase(status),
            status: status.as_u16(),
            detail: self.public_message(),
            kind: self.kind(),
            trace_id,
        };

        let mut response = (status, Json(body)).into_response();
        // RFC 7807: content-type MUST be application/problem+json.
        response.headers_mut().insert(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("application/problem+json"),
        );
        response
    }
}

fn reason_phrase(status: StatusCode) -> &'static str {
    status.canonical_reason().unwrap_or("Error")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_mapping_is_stable() {
        assert_eq!(AppError::NotFound.status(), StatusCode::NOT_FOUND);
        assert_eq!(AppError::Unauthorized.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(AppError::Forbidden.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            AppError::Validation("champ manquant".into()).status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            AppError::Conflict("déjà pris".into()).status(),
            StatusCode::CONFLICT
        );
        assert_eq!(AppError::RateLimited.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(AppError::Unavailable.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            AppError::Internal(anyhow::anyhow!("boom")).status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn kind_is_stable_and_snake_case() {
        for err in [
            AppError::NotFound,
            AppError::Unauthorized,
            AppError::Forbidden,
            AppError::Validation("x".into()),
            AppError::Conflict("x".into()),
            AppError::RateLimited,
            AppError::Unavailable,
            AppError::Internal(anyhow::anyhow!("x")),
        ] {
            let kind = err.kind();
            assert!(!kind.is_empty(), "kind must not be empty");
            assert!(
                kind.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "kind `{kind}` must be snake_case"
            );
        }
    }

    #[test]
    fn public_message_is_french_and_safe() {
        // The internal variant must never expose the source error.
        let err = AppError::Internal(anyhow::anyhow!("database password: hunter2"));
        let msg = err.public_message();
        assert!(!msg.contains("hunter2"));
        assert!(!msg.contains("password"));
        assert_eq!(msg, "Erreur interne du serveur.");
    }

    #[test]
    fn anyhow_conversion_wraps_as_internal() {
        let err: AppError = anyhow::anyhow!("boom").into();
        assert!(matches!(err, AppError::Internal(_)));
    }
}
