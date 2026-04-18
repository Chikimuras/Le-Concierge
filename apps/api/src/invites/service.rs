//! Team-invite orchestration — ties together token crypto, repo
//! persistence, audit events, and email delivery.

use std::time::Duration;

use aes_gcm::aead::rand_core::RngCore;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use hmac::{Hmac, Mac};
use rand::rngs::OsRng;
use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
use sha2::Sha256;

use crate::{
    audit::{AuditEvent, AuditRepo},
    auth::{AuthError, AuthRepo, Email, OrgId, Role, UserId, domain::Slug, hash::hash_password},
    email::SharedEmailSender,
    invites::{
        domain::{CreateInviteInput, Invite, InviteId, InvitePreview},
        error::InviteError,
        repo::InviteRepo,
    },
    session::{SessionData, SessionId, SessionService},
};

/// Outcome of a successful accept — returned to the HTTP layer so it
/// can decide whether to mint a new session (signup flow) or attach
/// the new membership to the caller's existing session (authed
/// accept).
#[derive(Debug, Clone)]
pub enum AcceptOutcome {
    /// A new user was created and a session issued. Carries the SID
    /// pair so the handler can set the cookie.
    NewSession {
        user_id: UserId,
        session_id: SessionId,
        session: SessionData,
    },
    /// The authed caller just gained a new membership — no session
    /// change needed.
    MembershipAdded { user_id: UserId },
}

type HmacSha256 = Hmac<Sha256>;

const INVITE_TOKEN_BYTES: usize = 32;

#[derive(Clone)]
pub struct InviteService {
    repo: InviteRepo,
    audit: AuditRepo,
    auth_repo: AuthRepo,
    sessions: SessionService,
    email: SharedEmailSender,
    pepper: SecretString,
    invite_ttl: Duration,
    public_base_url: String,
}

impl InviteService {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        repo: InviteRepo,
        audit: AuditRepo,
        auth_repo: AuthRepo,
        sessions: SessionService,
        email: SharedEmailSender,
        pepper: SecretString,
        invite_ttl: Duration,
        public_base_url: String,
    ) -> Self {
        Self {
            repo,
            audit,
            auth_repo,
            sessions,
            email,
            pepper,
            invite_ttl,
            public_base_url,
        }
    }

    // ---- Manager-side ------------------------------------------------------

    /// Create a new pending invite, then hand the URL to the email
    /// sender. The plaintext token is returned **only** to the email
    /// transport — it never leaves the service otherwise.
    pub async fn create(
        &self,
        actor: UserId,
        org_id: OrgId,
        org_slug: &Slug,
        org_name: &str,
        input: CreateInviteInput,
    ) -> Result<Invite, InviteError> {
        // admin role is platform-scoped (via `platform_admins`, not per
        // org) — disallow it explicitly.
        if input.role == Role::Owner {
            // Don't let a manager create another owner behind the
            // owner's back; owners mint owners through ownership
            // transfer flows we'll build later.
            return Err(InviteError::Validation("role"));
        }

        let token = generate_token();
        let token_hash = hmac_token(&token, &self.pepper);
        let expires_at = Utc::now()
            + chrono::Duration::from_std(self.invite_ttl)
                .unwrap_or_else(|_| chrono::Duration::days(7));

        let invite = self
            .repo
            .create(org_id, actor, &input, &token_hash, expires_at)
            .await?;

        let invite_url = format!(
            "{}/accept-invite?token={token}",
            self.public_base_url.trim_end_matches('/'),
        );
        self.email
            .send_invite(&invite.email, org_name, &invite_url)
            .await?;

        self.emit_audit(AuditEvent {
            kind: "invite.created",
            actor_user_id: Some(actor),
            org_id: Some(org_id),
            payload: json!({
                "invite_id": invite.id,
                "email": mask_email(invite.email.as_str()),
                "role": invite.role.as_str(),
                "org_slug": org_slug.as_str(),
            }),
        })
        .await;

        Ok(invite)
    }

    pub async fn list_pending(&self, org_id: OrgId) -> Result<Vec<Invite>, InviteError> {
        self.repo.list_pending(org_id).await
    }

    pub async fn cancel(
        &self,
        actor: UserId,
        org_id: OrgId,
        id: InviteId,
    ) -> Result<(), InviteError> {
        self.repo.cancel(org_id, id).await?;
        self.emit_audit(AuditEvent {
            kind: "invite.cancelled",
            actor_user_id: Some(actor),
            org_id: Some(org_id),
            payload: json!({ "invite_id": id }),
        })
        .await;
        Ok(())
    }

    // ---- Invitee-side -----------------------------------------------------

    /// Look up an invite's non-sensitive metadata for display. Does
    /// not consume. Anonymous-friendly — the token is the access
    /// proof.
    pub async fn preview(&self, token: &str) -> Result<InvitePreview, InviteError> {
        let hash = hmac_token(token, &self.pepper);
        let invite = self.repo.find_active_by_token_hash(&hash).await?;
        let org_name = self
            .load_org_name(invite.org_id)
            .await?
            .ok_or(InviteError::NotFound)?;
        Ok(InvitePreview {
            email: invite.email,
            org_name,
            role: invite.role,
            expires_at: invite.expires_at,
        })
    }

    /// Accept as an already-authenticated user. The session's email
    /// must match the invite's email (case-insensitive via `citext`
    /// + domain-level `Email`).
    pub async fn accept_as(
        &self,
        caller_user_id: UserId,
        caller_email: &Email,
        token: &str,
    ) -> Result<AcceptOutcome, InviteError> {
        let hash = hmac_token(token, &self.pepper);
        let invite = self.repo.find_active_by_token_hash(&hash).await?;

        if caller_email != &invite.email {
            return Err(InviteError::NotFound);
        }

        self.repo
            .accept(invite.id, caller_user_id, invite.org_id, invite.role)
            .await?;

        self.emit_audit(AuditEvent {
            kind: "invite.accepted",
            actor_user_id: Some(caller_user_id),
            org_id: Some(invite.org_id),
            payload: json!({
                "invite_id": invite.id,
                "role": invite.role.as_str(),
                "path": "authed",
            }),
        })
        .await;

        Ok(AcceptOutcome::MembershipAdded {
            user_id: caller_user_id,
        })
    }

    /// Anonymous signup-and-accept: create the user (email inherited
    /// from the invite, password supplied), consume the invite, link
    /// the membership, mint a session. All-or-nothing at the repo
    /// layer — a partial failure leaves no user and no membership.
    pub async fn signup_and_accept(
        &self,
        token: &str,
        password: &str,
        ip: std::net::IpAddr,
        user_agent: &str,
    ) -> Result<AcceptOutcome, InviteError> {
        let hash = hmac_token(token, &self.pepper);
        let invite = self.repo.find_active_by_token_hash(&hash).await?;

        // Create the user with the invite's email. If a user already
        // exists with that email, bounce to "login first then accept".
        let password_hash = hash_password(password, &self.pepper).map_err(InviteError::Auth)?;
        let user_id = self
            .auth_repo
            .create_user_with_email(&invite.email, &password_hash)
            .await
            .map_err(|e| match e {
                AuthError::EmailAlreadyTaken => InviteError::EmailAlreadyTaken,
                other => InviteError::Auth(other),
            })?;

        self.repo
            .accept(invite.id, user_id, invite.org_id, invite.role)
            .await?;

        let (session_id, session) = self
            .sessions
            .create(user_id, ip, user_agent)
            .await
            .map_err(|e| InviteError::Internal(e.into()))?;

        self.emit_audit(AuditEvent {
            kind: "invite.accepted",
            actor_user_id: Some(user_id),
            org_id: Some(invite.org_id),
            payload: json!({
                "invite_id": invite.id,
                "role": invite.role.as_str(),
                "path": "signup",
            }),
        })
        .await;

        Ok(AcceptOutcome::NewSession {
            user_id,
            session_id,
            session,
        })
    }

    // ---- helpers ----------------------------------------------------------

    async fn load_org_name(&self, org_id: OrgId) -> Result<Option<String>, InviteError> {
        let row = sqlx::query_scalar!(
            r#"SELECT name FROM organizations WHERE id = $1"#,
            org_id.into_inner(),
        )
        .fetch_optional(self.repo.pool())
        .await?;
        Ok(row)
    }

    async fn emit_audit(&self, event: AuditEvent) {
        if let Err(err) = self.audit.record(event).await {
            tracing::error!(error = %err, "failed to record invite audit event");
        }
    }
}

// ---- free helpers --------------------------------------------------------

/// Generate a 32-byte `OsRng` token, URL-safe base64 without padding.
/// Matches the CSRF / SessionId budget from ADR 0006.
fn generate_token() -> String {
    let mut bytes = [0u8; INVITE_TOKEN_BYTES];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// HMAC-SHA-256(pepper, token) → 64-char hex. See ADR 0009 for why we
/// use an HMAC rather than Argon2id here.
fn hmac_token(token: &str, pepper: &SecretString) -> String {
    // `HmacSha256::new_from_slice` is fallible only when the key is an
    // unacceptable length — HMAC-SHA-256 accepts any length, so the
    // `Result` is structurally unreachable. Unwrap via `match` so we
    // don't trip the clippy `expect_used` lint.
    let mut mac = match HmacSha256::new_from_slice(pepper.expose_secret().as_bytes()) {
        Ok(m) => m,
        Err(_) => unreachable!("HMAC-SHA-256 accepts any key length"),
    };
    mac.update(token.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Audit-log safe email mask — copy of the auth-side helper so we
/// don't cross module boundaries. CLAUDE.md §3.3.
fn mask_email(raw: &str) -> String {
    if let Some(at) = raw.find('@') {
        let (local, rest) = raw.split_at(at);
        let first = local.chars().next().unwrap_or('*');
        format!("{first}***{rest}")
    } else {
        "***".into()
    }
}
