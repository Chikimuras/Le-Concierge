//! Test-only [`EmailSender`] stubs. The production senders live under
//! `api::email`; these stand in for them when the test wants a specific
//! failure mode that real SMTP / log senders do not exercise.

use api::{
    auth::domain::Email,
    email::{EmailError, EmailSender},
};
use async_trait::async_trait;

/// Always returns [`EmailError::Transport`]. Used to assert the
/// invite-create fail-closed rollback path.
#[derive(Debug, Default, Clone)]
pub struct FailingEmailSender;

#[async_trait]
impl EmailSender for FailingEmailSender {
    async fn send_invite(
        &self,
        _to: &Email,
        _org_name: &str,
        _invite_url: &str,
    ) -> Result<(), EmailError> {
        Err(EmailError::Transport("stub: refused".into()))
    }
}
