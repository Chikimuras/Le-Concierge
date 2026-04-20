//! Email delivery abstraction.
//!
//! A single async trait fronts whatever transport we end up plugging
//! in (Resend, Postmark, raw SMTP, …). Phase 5b-1 shipped
//! [`LogEmailSender`]; Phase 5b-2bis adds [`SmtpEmailSender`] behind
//! `APP_EMAIL__MODE=smtp` so dev invites land in Mailpit instead of
//! stdout. Prod will swap the SMTP path for a provider API client
//! (ADR 0010).

use std::sync::Arc;

use async_trait::async_trait;

use crate::auth::domain::Email;
use crate::config::{EmailConfig, EmailMode};

pub mod log_sender;
pub mod smtp_sender;

pub use log_sender::LogEmailSender;
pub use smtp_sender::SmtpEmailSender;

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

/// Build the [`SharedEmailSender`] selected by `config.email.mode`.
///
/// `log` mode is infallible — it just stores the unit struct. `smtp`
/// requires `smtp_host`, `smtp_port`, and `from_address` to all be
/// present, and the from-address must parse as a valid RFC 5321
/// mailbox; anything else is a configuration error that fails boot
/// (CLAUDE.md §9 — fail closed when a dependency is misconfigured).
///
/// # Errors
///
/// Returns an error if `smtp` mode is selected without the required
/// fields, or if `from_address` is not a valid mailbox.
pub fn build_sender(config: &EmailConfig) -> anyhow::Result<SharedEmailSender> {
    match config.mode {
        EmailMode::Log => Ok(Arc::new(LogEmailSender)),
        EmailMode::Smtp => {
            let host = config.smtp_host.as_deref().ok_or_else(|| {
                anyhow::anyhow!("APP_EMAIL__SMTP_HOST is required when mode=smtp")
            })?;
            let port = config.smtp_port.ok_or_else(|| {
                anyhow::anyhow!("APP_EMAIL__SMTP_PORT is required when mode=smtp")
            })?;
            let from_address = config.from_address.as_deref().ok_or_else(|| {
                anyhow::anyhow!("APP_EMAIL__FROM_ADDRESS is required when mode=smtp")
            })?;
            let sender =
                SmtpEmailSender::new(host, port, from_address, config.from_name.as_deref())?;
            Ok(Arc::new(sender))
        }
    }
}
