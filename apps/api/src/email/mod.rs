//! Email delivery abstraction.
//!
//! A single async trait fronts whatever transport we end up plugging
//! in (Resend, Postmark, raw SMTP, …). Phase 5b-1 ships
//! [`LogEmailSender`], which just logs the message to `tracing::info!`
//! — enough for solo dev + E2E tests, not enough for production. A
//! later ADR introduces the real senders.

use std::sync::Arc;

use async_trait::async_trait;

use crate::auth::domain::Email;

pub mod log_sender;

pub use log_sender::LogEmailSender;

/// Shared handle injected into `AppState`. Cheap to clone.
pub type SharedEmailSender = Arc<dyn EmailSender>;

/// Errors surfaced by transports. `Internal` is the catch-all — the
/// caller treats every variant as "couldn't deliver" and the service
/// decides whether to fail closed or best-effort.
#[derive(Debug, thiserror::Error)]
pub enum EmailError {
    #[error("transport refused the message: {0}")]
    Transport(String),
    #[error(transparent)]
    Internal(anyhow::Error),
}

/// Minimal surface for the invite flow. Intentionally narrow — we
/// extend it only when a new template lands, not speculatively.
#[async_trait]
pub trait EmailSender: Send + Sync {
    async fn send_invite(
        &self,
        to: &Email,
        org_name: &str,
        invite_url: &str,
    ) -> Result<(), EmailError>;
}
