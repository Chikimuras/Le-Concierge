//! High-level orchestration for the `auth` domain.
//!
//! Intended to be the only entry point used by HTTP handlers (and, later,
//! the CLI). Keeps the domain / repo layers pure; every policy decision
//! (password strength, rate limits, audit logging, session creation) lives
//! here.
//!
//! Phase 4a ships only `signup_organization`. Additional flows (login,
//! logout, 2FA) land in Phases 4b/4c.

use secrecy::SecretString;

use crate::auth::{
    domain::{Email, OrgId, Slug, UserId},
    error::AuthError,
    hash::hash_password,
    repo::AuthRepo,
};

/// Input to [`AuthService::signup_organization`]. Raw strings are accepted
/// and validated here so callers don't need to build newtypes themselves.
#[derive(Debug)]
pub struct SignupInput {
    pub email: String,
    pub password: String,
    pub organization_slug: String,
    pub organization_name: String,
}

/// Result of a successful signup. HTTP handlers map this into a DTO; the
/// CLI can consume it directly.
#[derive(Debug, Clone, Copy)]
pub struct SignupOutcome {
    pub organization_id: OrgId,
    pub owner_user_id: UserId,
}

/// Orchestration over [`AuthRepo`] and the hashing primitives. Cheap to
/// clone (holds an `Arc` pool and a `SecretString`, itself `Arc`-backed).
#[derive(Clone)]
pub struct AuthService {
    repo: AuthRepo,
    pepper: SecretString,
}

impl AuthService {
    #[must_use]
    pub fn new(repo: AuthRepo, pepper: SecretString) -> Self {
        Self { repo, pepper }
    }

    /// Create a brand-new organization together with its first user (the
    /// owner). Validation, hashing, and persistence happen in one flow:
    ///
    /// 1. Parse + validate email and slug (rejects invalid input before
    ///    touching the DB).
    /// 2. Hash the password with Argon2id + pepper (CLAUDE.md §3.1).
    /// 3. Insert organization, user, and `owner` membership in a single
    ///    transaction.
    ///
    /// Audit event emission happens in a later phase once the hash-chain
    /// primitive is wired up.
    ///
    /// # Errors
    ///
    /// - [`AuthError::InvalidEmail`] / [`AuthError::InvalidSlug`] —
    ///   client-supplied value failed validation.
    /// - [`AuthError::WeakPassword`] — password is shorter than
    ///   [`crate::auth::hash::MIN_PASSWORD_LEN`].
    /// - [`AuthError::EmailAlreadyTaken`] / [`AuthError::SlugAlreadyTaken`] —
    ///   unique constraint violation at the DB layer.
    /// - [`AuthError::Repository`] / [`AuthError::Hashing`] for internal
    ///   failures.
    pub async fn signup_organization(
        &self,
        input: SignupInput,
    ) -> Result<SignupOutcome, AuthError> {
        let email = Email::parse(&input.email)?;
        let slug = Slug::parse(&input.organization_slug)?;

        // Validate the org name separately from slug.
        let name = input.organization_name.trim();
        if name.is_empty() || name.len() > 200 {
            return Err(AuthError::InvalidSlug);
        }

        let password_hash = hash_password(&input.password, &self.pepper)?;

        let (organization_id, owner_user_id) = self
            .repo
            .create_organization_with_owner(&email, &password_hash, &slug, name)
            .await?;

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
}
