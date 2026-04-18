//! Dev-only [`EmailSender`][super::EmailSender] that logs each
//! message instead of sending it.
//!
//! The invite URL contains a live 43-char token — logs are the only
//! place this plaintext ever appears outside the user's inbox, which
//! is exactly what we want locally (the solo dev reads their own
//! terminal), and exactly what we do **not** want in production. A
//! deployment that keeps this wired past Phase 5b-1 is a bug; a
//! `warn!` on the first call nudges operators toward the real sender.

use async_trait::async_trait;

use super::{EmailError, EmailSender};
use crate::auth::domain::Email;

#[derive(Debug, Default, Clone)]
pub struct LogEmailSender;

#[async_trait]
impl EmailSender for LogEmailSender {
    async fn send_invite(
        &self,
        to: &Email,
        org_name: &str,
        invite_url: &str,
    ) -> Result<(), EmailError> {
        tracing::warn!(
            target = "dev_email",
            to = %mask_email(to.as_str()),
            org = %org_name,
            invite_url = %invite_url,
            "dev-mode email sender — configure APP_EMAIL__MODE=resend before going to prod",
        );
        Ok(())
    }
}

/// Local copy of the auth-side mask_email helper. We cannot import it
/// directly because it lives in an `auth::service` private module.
fn mask_email(raw: &str) -> String {
    if let Some(at) = raw.find('@') {
        let (local, rest) = raw.split_at(at);
        let first = local.chars().next().unwrap_or('*');
        format!("{first}***{rest}")
    } else {
        "***".into()
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn log_sender_never_fails() {
        let sender = LogEmailSender;
        let to = Email::parse("alice@example.test").expect("email");
        sender
            .send_invite(&to, "Acme", "http://localhost/accept-invite?token=xxx")
            .await
            .expect("log sender is infallible");
    }

    #[test]
    fn mask_email_hides_local_part() {
        assert_eq!(mask_email("alice@example.com"), "a***@example.com");
        assert_eq!(mask_email("no-at-sign"), "***");
    }
}
