//! High-level orchestration for the `auth` domain.
//!
//! The HTTP layer (and, later, the CLI) consumes this module — policy
//! decisions live here, primitives live in [`super::hash`] / [`super::repo`].
//!
//! Audit events are emitted best-effort *after* the business mutation.
//! They share a transaction only with their originating write when the
//! operation is atomic by construction (e.g. signup); for login and
//! logout the tiny window during which an audit insert could lag the
//! mutation is accepted and documented in ADR 0006.

use std::net::IpAddr;

use chrono::Utc;
use secrecy::SecretString;
use serde_json::json;

use crate::{
    audit::{AuditEvent, AuditRepo},
    auth::{
        domain::{Email, OrgId, Slug, UserId},
        error::AuthError,
        hash::{hash_password, verify_password},
        repo::{AuthRepo, MembershipRow},
    },
    session::{SessionData, SessionId, SessionService},
};

/// Input to [`AuthService::signup_organization`].
#[derive(Debug)]
pub struct SignupInput {
    pub email: String,
    pub password: String,
    pub organization_slug: String,
    pub organization_name: String,
}

/// Result of a successful signup.
#[derive(Debug, Clone, Copy)]
pub struct SignupOutcome {
    pub organization_id: OrgId,
    pub owner_user_id: UserId,
}

/// Input to [`AuthService::login_with_password`].
#[derive(Debug)]
pub struct LoginInput {
    pub email: String,
    pub password: String,
}

/// Pair of session identifier + data returned by login / signup. The
/// caller is responsible for setting the cookie.
#[derive(Debug, Clone)]
pub struct SessionIssue {
    pub session_id: SessionId,
    pub session: SessionData,
}

/// Everything a logged-in view needs about the current user beyond the
/// session itself. Hydrated by [`AuthService::load_user_context`].
#[derive(Debug, Clone)]
pub struct UserContext {
    pub memberships: Vec<MembershipRow>,
    pub is_platform_admin: bool,
}

/// Orchestration over [`AuthRepo`], [`AuditRepo`], and [`SessionService`].
/// Cheap to clone.
#[derive(Clone)]
pub struct AuthService {
    repo: AuthRepo,
    audit: AuditRepo,
    sessions: SessionService,
    pepper: SecretString,
}

impl AuthService {
    #[must_use]
    pub fn new(
        repo: AuthRepo,
        audit: AuditRepo,
        sessions: SessionService,
        pepper: SecretString,
    ) -> Self {
        Self {
            repo,
            audit,
            sessions,
            pepper,
        }
    }

    /// Create an organization and its first user (owner). Audit event
    /// `auth.signup` is emitted on success (best-effort).
    pub async fn signup_organization(
        &self,
        input: SignupInput,
    ) -> Result<SignupOutcome, AuthError> {
        let email = Email::parse(&input.email)?;
        let slug = Slug::parse(&input.organization_slug)?;

        let name = input.organization_name.trim();
        if name.is_empty() || name.len() > 200 {
            return Err(AuthError::InvalidSlug);
        }

        let password_hash = hash_password(&input.password, &self.pepper)?;

        let (organization_id, owner_user_id) = self
            .repo
            .create_organization_with_owner(&email, &password_hash, &slug, name)
            .await?;

        self.emit_audit(AuditEvent {
            kind: "auth.signup",
            actor_user_id: Some(owner_user_id),
            org_id: Some(organization_id),
            payload: json!({
                "email": mask_email(&email),
                "slug": slug.as_str(),
            }),
        })
        .await;

        tracing::info!(
            org_id = %organization_id,
            user_id = %owner_user_id,
            "organization created with owner"
        );

        Ok(SignupOutcome {
            organization_id,
            owner_user_id,
        })
    }

    /// Verify a user's credentials and, on success, create a fresh
    /// server-side session. Applies the progressive lockout policy and
    /// emits `auth.login.success` or `auth.login.failure` audit events.
    ///
    /// Callers always see [`AuthError::InvalidCredentials`] regardless of
    /// whether the email was unknown, the password wrong, or the account
    /// locked — this preserves the opacity property required by OWASP
    /// ASVS 2.1.1.
    pub async fn login_with_password(
        &self,
        input: LoginInput,
        ip: IpAddr,
        user_agent: &str,
    ) -> Result<(UserId, SessionIssue), AuthError> {
        let email = match Email::parse(&input.email) {
            Ok(e) => e,
            Err(_) => {
                // Enumeration-safe: do not surface "invalid email" on a
                // login form — looks identical to a wrong password.
                self.emit_audit(failure_audit(None, ip, &input.email)).await;
                return Err(AuthError::InvalidCredentials);
            }
        };

        let Some(user) = self.repo.find_user_by_email(&email).await? else {
            self.emit_audit(failure_audit(None, ip, &input.email)).await;
            return Err(AuthError::InvalidCredentials);
        };

        // Lockout check first, still generic at the client level.
        if let Some(until) = user.locked_until
            && until > Utc::now()
        {
            self.emit_audit(failure_audit(Some(user.id), ip, &input.email))
                .await;
            return Err(AuthError::InvalidCredentials);
        }

        let matches = verify_password(&input.password, &self.pepper, &user.password_hash)?;

        if !matches {
            let _ = self.repo.record_failed_login(user.id).await;
            self.emit_audit(failure_audit(Some(user.id), ip, &input.email))
                .await;
            return Err(AuthError::InvalidCredentials);
        }

        // Success — reset counter, mint session, emit audit.
        self.repo.reset_failed_logins(user.id).await?;

        let (session_id, session) = match self.sessions.create(user.id, ip, user_agent).await {
            Ok(pair) => pair,
            Err(e) => return Err(AuthError::Internal(e.into())),
        };

        self.emit_audit(AuditEvent {
            kind: "auth.login.success",
            actor_user_id: Some(user.id),
            org_id: None,
            payload: json!({
                "ip": crate::session::dto::mask_ip(ip),
                "ua": crate::session::dto::fingerprint_user_agent(user_agent),
            }),
        })
        .await;

        Ok((
            user.id,
            SessionIssue {
                session_id,
                session,
            },
        ))
    }

    /// Destroy a live session and emit `auth.logout`.
    pub async fn logout(&self, session_id: &SessionId, user_id: UserId) -> Result<(), AuthError> {
        self.sessions
            .destroy(session_id)
            .await
            .map_err(|e| AuthError::Internal(e.into()))?;

        self.emit_audit(AuditEvent {
            kind: "auth.logout",
            actor_user_id: Some(user_id),
            org_id: None,
            payload: json!({}),
        })
        .await;

        Ok(())
    }

    /// Expose the inner [`SessionService`] so routes can reach the
    /// cookie-building helpers without holding a second state field.
    #[must_use]
    pub fn sessions(&self) -> &SessionService {
        &self.sessions
    }

    /// Hydrate the membership list and platform-admin flag for a user.
    /// Used by `/auth/me`, `/auth/login`, and `/auth/signup` response
    /// bodies so the frontend can bootstrap its role-gated UI in one
    /// round-trip.
    pub async fn load_user_context(&self, user_id: UserId) -> Result<UserContext, AuthError> {
        let memberships = self.repo.list_memberships(user_id).await?;
        let is_platform_admin = self.repo.is_platform_admin(user_id).await?;
        Ok(UserContext {
            memberships,
            is_platform_admin,
        })
    }

    async fn emit_audit(&self, event: AuditEvent) {
        if let Err(err) = self.audit.record(event).await {
            tracing::error!(error = %err, "failed to record audit event");
        }
    }
}

/// Build a login-failure audit event. `email` is truncated to its first
/// three characters so we never log a raw identifier (CLAUDE.md §3.3).
fn failure_audit(actor: Option<UserId>, ip: IpAddr, email: &str) -> AuditEvent {
    let prefix: String = email.chars().take(3).collect();
    AuditEvent {
        kind: "auth.login.failure",
        actor_user_id: actor,
        org_id: None,
        payload: json!({
            "ip": crate::session::dto::mask_ip(ip),
            "email_prefix": prefix,
        }),
    }
}

/// Mask an email address for audit payloads: first character + `***@` +
/// domain. Avoids logging user-provided identifiers directly.
fn mask_email(email: &Email) -> String {
    let s = email.as_str();
    if let Some(at) = s.find('@') {
        let (local, rest) = s.split_at(at);
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
    fn mask_email_hides_local_part() {
        let e = Email::parse("alice@example.com").expect("valid");
        assert_eq!(mask_email(&e), "a***@example.com");
    }
}
