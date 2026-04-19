//! [`EmailSender`][super::EmailSender] backed by an async SMTP client.
//!
//! Designed for local dev against Mailpit (`127.0.0.1:1025`, no auth,
//! no TLS) per ADR 0010. The transport is built without credentials and
//! without a TLS wrapper on purpose — real providers in prod will
//! bypass this sender entirely and call a REST API (Resend / Postmark).
//!
//! Messages are rendered as a tiny inline HTML document plus a plain
//! text alternative; this keeps the dependency graph narrow (no
//! MJML / Tera) and means there is exactly one place to audit for
//! injection issues: the `build_message` helper below.

use async_trait::async_trait;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Mailbox, MultiPart, SinglePart, header::ContentType},
    transport::smtp::client::Tls,
};

use super::{EmailError, EmailSender};
use crate::auth::domain::Email;

/// Async SMTP sender. Cheap to clone: the underlying `AsyncSmtpTransport`
/// pools connections internally.
#[derive(Debug, Clone)]
pub struct SmtpEmailSender {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
}

impl SmtpEmailSender {
    /// Build a sender pointed at `host:port`, tagging every outgoing
    /// message with the given from-address / display name.
    ///
    /// # Errors
    ///
    /// Returns an error if `from_address` is not a syntactically valid
    /// RFC 5321 mailbox, or if the transport builder rejects the host.
    pub fn new(
        host: &str,
        port: u16,
        from_address: &str,
        from_name: Option<&str>,
    ) -> anyhow::Result<Self> {
        let address = from_address.parse().map_err(|e| {
            anyhow::anyhow!("invalid APP_EMAIL__FROM_ADDRESS {from_address:?}: {e}")
        })?;
        let from = Mailbox::new(from_name.map(ToOwned::to_owned), address);

        // `builder_dangerous` = plaintext SMTP, no STARTTLS. Accepted
        // here because we only ever dial 127.0.0.1 / the compose
        // network. The production swap point is a different sender
        // entirely (ADR 0010).
        let transport = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host)
            .port(port)
            .tls(Tls::None)
            .build();

        Ok(Self { transport, from })
    }
}

#[async_trait]
impl EmailSender for SmtpEmailSender {
    async fn send_invite(
        &self,
        to: &Email,
        org_name: &str,
        invite_url: &str,
    ) -> Result<(), EmailError> {
        let to_address = to
            .as_str()
            .parse()
            .map_err(|e| EmailError::Internal(anyhow::anyhow!("invalid recipient: {e}")))?;
        let to_mailbox = Mailbox::new(None, to_address);

        let message = build_message(&self.from, to_mailbox, org_name, invite_url)
            .map_err(|e| EmailError::Internal(anyhow::anyhow!("build message: {e}")))?;

        self.transport
            .send(message)
            .await
            .map_err(|e| EmailError::Transport(e.to_string()))?;

        tracing::info!(
            target = "email_sender",
            to = %mask_email(to.as_str()),
            org = %org_name,
            "sent invite via smtp",
        );
        Ok(())
    }
}

/// Compose the `Message` for an invite. HTML renders the URL both as a
/// button label and as the `href`; the text alternative carries the
/// same URL so MUAs that strip HTML still get a usable link.
fn build_message(
    from: &Mailbox,
    to: Mailbox,
    org_name: &str,
    invite_url: &str,
) -> anyhow::Result<Message> {
    let subject = format!("Invitation — {org_name}");
    let text_body = format!(
        "Vous avez été invité·e à rejoindre {org_name} sur Le Concierge.\n\
         \n\
         Ouvrez ce lien pour accepter l'invitation :\n\
         {invite_url}\n\
         \n\
         Si vous n'attendiez pas cette invitation, ignorez ce message.",
    );
    let html_body = render_html(org_name, invite_url);

    let message = Message::builder()
        .from(from.clone())
        .to(to)
        .subject(subject)
        .multipart(
            MultiPart::alternative()
                .singlepart(
                    SinglePart::builder()
                        .header(ContentType::TEXT_PLAIN)
                        .body(text_body),
                )
                .singlepart(
                    SinglePart::builder()
                        .header(ContentType::TEXT_HTML)
                        .body(html_body),
                ),
        )?;
    Ok(message)
}

/// Render the HTML body. Both interpolations are HTML-escaped — the
/// org name is user-controlled (tenant may pick anything) and the
/// invite URL is URL-safe but still worth escaping for `&` inside
/// query strings. Keep this helper pure so it stays testable.
fn render_html(org_name: &str, invite_url: &str) -> String {
    let org = html_escape(org_name);
    let url = html_escape(invite_url);
    format!(
        "<!doctype html>\
         <html lang=\"fr\"><body style=\"font-family:system-ui,sans-serif;color:#111;\">\
         <p>Vous avez été invité·e à rejoindre <strong>{org}</strong> sur Le Concierge.</p>\
         <p><a href=\"{url}\" style=\"display:inline-block;padding:10px 16px;background:#111;color:#fff;text-decoration:none;border-radius:6px;\">Accepter l'invitation</a></p>\
         <p style=\"font-size:12px;color:#666;\">Ou copiez ce lien dans votre navigateur :<br><code>{url}</code></p>\
         <p style=\"font-size:12px;color:#666;\">Si vous n'attendiez pas cette invitation, ignorez ce message.</p>\
         </body></html>",
    )
}

/// Bare-minimum HTML escape. Enough for attribute values + text nodes
/// in the inline template we emit; we do not render arbitrary markup.
fn html_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
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

    #[test]
    fn html_escape_covers_the_five_entities() {
        assert_eq!(
            html_escape("<a href=\"x&y\">'t</a>"),
            "&lt;a href=&quot;x&amp;y&quot;&gt;&#39;t&lt;/a&gt;",
        );
    }

    #[test]
    fn render_html_escapes_tenant_name_and_url() {
        let html = render_html(
            "Acme & Co <injected>",
            "http://localhost/accept-invite?token=abc&x=1",
        );
        assert!(html.contains("Acme &amp; Co &lt;injected&gt;"));
        assert!(html.contains("token=abc&amp;x=1"));
        assert!(!html.contains("<injected>"));
    }

    #[test]
    fn build_message_accepts_valid_inputs() {
        let from = Mailbox::new(
            Some("Dev".into()),
            "dev@leconcierge.test".parse().expect("valid"),
        );
        let to = Mailbox::new(None, "alice@example.test".parse().expect("valid"));
        let message = build_message(
            &from,
            to,
            "Acme",
            "http://localhost/accept-invite?token=xxx",
        )
        .expect("message builds");
        // The message serialises to RFC 5322 on the wire; spot-check
        // that the subject survived templating.
        let bytes = message.formatted();
        let raw = String::from_utf8_lossy(&bytes);
        assert!(raw.contains("Subject: Invitation"));
        assert!(raw.contains("Acme"));
    }

    #[test]
    fn new_rejects_malformed_from_address() {
        let err = SmtpEmailSender::new("127.0.0.1", 1025, "not-an-email", None)
            .expect_err("should reject");
        assert!(err.to_string().contains("not-an-email"));
    }

    #[test]
    fn mask_email_hides_local_part() {
        assert_eq!(mask_email("alice@example.com"), "a***@example.com");
        assert_eq!(mask_email("no-at-sign"), "***");
    }

    /// End-to-end smoke against a running Mailpit on 127.0.0.1:1025.
    /// Gated behind `--ignored` because CI does not run the compose
    /// stack — only dev machines do.
    #[tokio::test]
    #[ignore = "requires `just compose-up` (mailpit on 127.0.0.1:1025)"]
    async fn smoke_sends_to_mailpit() {
        let sender = SmtpEmailSender::new(
            "127.0.0.1",
            1025,
            "dev@leconcierge.test",
            Some("Le Concierge (test)"),
        )
        .expect("sender builds");
        let to = Email::parse("alice@example.test").expect("email");
        sender
            .send_invite(&to, "Acme", "http://localhost/accept-invite?token=smoke")
            .await
            .expect("mailpit accepts");
    }
}
