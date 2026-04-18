//! TOTP enrollment, verification, and disable orchestration.
//!
//! The service is purely *auth-domain* logic — session rotation on verify
//! happens in the HTTP route (which has access to [`SessionService`]).
//! Audit emission is best-effort, mirroring the password auth flow
//! (ADR 0006).

use std::{
    net::IpAddr,
    time::{SystemTime, UNIX_EPOCH},
};

use base32::Alphabet;
use secrecy::SecretString;
use serde_json::json;

use crate::{
    audit::{AuditEvent, AuditRepo},
    auth::{
        domain::{Email, UserId},
        error::AuthError,
        totp::{
            codes,
            crypto::{self, TotpEncryptionKey},
            domain::{RecoveryCode, TOTP_SECRET_CIPHER_LEN},
            generator,
            repo::TotpRepo,
        },
    },
};

/// What [`TotpService::start_enrollment`] returns for the frontend to
/// render. `otpauth_url` feeds a QR generator; `secret_base32` covers
/// the manual-entry fallback for users without a camera.
#[derive(Debug, Clone)]
pub struct TotpEnrollmentStart {
    pub otpauth_url: String,
    pub secret_base32: String,
}

/// Outcome variant of a successful `verify_code` — the HTTP layer turns
/// this into the response body / audit event so the UI can warn the user
/// ("you just burned a recovery code, consider re-enrolling").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TotpVerifyOutcome {
    Totp,
    RecoveryCode,
}

#[derive(Clone)]
pub struct TotpService {
    repo: TotpRepo,
    audit: AuditRepo,
    pepper: SecretString,
    encryption_key: TotpEncryptionKey,
}

impl TotpService {
    #[must_use]
    pub fn new(
        repo: TotpRepo,
        audit: AuditRepo,
        pepper: SecretString,
        encryption_key: TotpEncryptionKey,
    ) -> Self {
        Self {
            repo,
            audit,
            pepper,
            encryption_key,
        }
    }

    /// Begin a fresh 2FA enrollment. Generates a 20-byte secret, wraps it
    /// with AES-GCM, persists it as *pending* (not yet verified), and
    /// returns the material the frontend needs to render a QR.
    ///
    /// If the user is already enrolled (active, not disabled), returns
    /// [`AuthError::TotpAlreadyEnrolled`] — they must disable first.
    pub async fn start_enrollment(
        &self,
        user_id: UserId,
        email: &Email,
        ip: IpAddr,
        user_agent: &str,
    ) -> Result<TotpEnrollmentStart, AuthError> {
        let secret = crypto::generate_secret();
        let cipher = crypto::encrypt_secret(&self.encryption_key, &secret)?;
        self.repo
            .upsert_pending_enrollment(user_id, &cipher)
            .await?;

        let otpauth_url = generator::otpauth_url(email, &secret)?;
        let secret_base32 = base32::encode(Alphabet::Rfc4648 { padding: false }, secret.as_bytes());

        self.emit_audit(AuditEvent {
            kind: "auth.2fa.enroll.start",
            actor_user_id: Some(user_id),
            org_id: None,
            payload: audit_context(ip, user_agent),
        })
        .await;

        Ok(TotpEnrollmentStart {
            otpauth_url,
            secret_base32,
        })
    }

    /// Finalise an enrollment: verify the first TOTP code produced by the
    /// authenticator, promote the row from pending to active, generate
    /// the 10 recovery codes, and return their plaintext **once**.
    pub async fn confirm_enrollment(
        &self,
        user_id: UserId,
        email: &Email,
        submitted_code: &str,
        ip: IpAddr,
        user_agent: &str,
    ) -> Result<Vec<RecoveryCode>, AuthError> {
        let secret_cipher = self
            .repo
            .fetch_pending_secret(user_id)
            .await?
            .ok_or(AuthError::TotpNotEnrolled)?;
        let secret = decrypt_fixed(&self.encryption_key, &secret_cipher)?;

        if !generator::verify_code(email, &secret, unix_now()?, submitted_code)? {
            self.emit_audit(AuditEvent {
                kind: "auth.2fa.verify.failure",
                actor_user_id: Some(user_id),
                org_id: None,
                payload: {
                    let mut p = audit_context(ip, user_agent);
                    p["reason"] = json!("enroll_wrong_code");
                    p
                },
            })
            .await;
            return Err(AuthError::TotpInvalidCode);
        }

        let codes = codes::generate_recovery_codes(&self.pepper)?;
        let hashes: Vec<_> = codes.iter().map(|(_, h)| h.clone()).collect();
        self.repo.confirm_enrollment(user_id, &hashes).await?;

        self.emit_audit(AuditEvent {
            kind: "auth.2fa.enroll.success",
            actor_user_id: Some(user_id),
            org_id: None,
            payload: audit_context(ip, user_agent),
        })
        .await;

        Ok(codes.into_iter().map(|(c, _)| c).collect())
    }

    /// Verify a step-up submission. Tries the TOTP code first, falls
    /// back to recovery codes. On success, any consumed recovery code is
    /// marked used inside this call.
    pub async fn verify_code(
        &self,
        user_id: UserId,
        email: &Email,
        submitted: &str,
        ip: IpAddr,
        user_agent: &str,
    ) -> Result<TotpVerifyOutcome, AuthError> {
        let secret_cipher = self
            .repo
            .fetch_active_secret(user_id)
            .await?
            .ok_or(AuthError::TotpNotEnrolled)?;
        let secret = decrypt_fixed(&self.encryption_key, &secret_cipher)?;

        if generator::verify_code(email, &secret, unix_now()?, submitted)? {
            self.emit_audit(AuditEvent {
                kind: "auth.2fa.verify.success",
                actor_user_id: Some(user_id),
                org_id: None,
                payload: {
                    let mut p = audit_context(ip, user_agent);
                    p["method"] = json!("totp");
                    p
                },
            })
            .await;
            return Ok(TotpVerifyOutcome::Totp);
        }

        let unused = self.repo.list_unused_recovery_codes(user_id).await?;
        let hashes: Vec<_> = unused.iter().map(|r| r.code_hash.clone()).collect();
        if let Some(idx) = codes::verify_recovery_code(submitted, &self.pepper, &hashes)?
            && self.repo.mark_recovery_code_used(unused[idx].id).await?
        {
            self.emit_audit(AuditEvent {
                kind: "auth.2fa.recovery.used",
                actor_user_id: Some(user_id),
                org_id: None,
                payload: audit_context(ip, user_agent),
            })
            .await;
            return Ok(TotpVerifyOutcome::RecoveryCode);
        }

        self.emit_audit(AuditEvent {
            kind: "auth.2fa.verify.failure",
            actor_user_id: Some(user_id),
            org_id: None,
            payload: {
                let mut p = audit_context(ip, user_agent);
                p["reason"] = json!("wrong_code");
                p
            },
        })
        .await;
        Err(AuthError::TotpInvalidCode)
    }

    /// Disable 2FA for a user. Caller is responsible for re-verifying
    /// the password (via `AuthService::verify_password_for_user`) and
    /// passes a freshly validated TOTP code here. Deletes both the
    /// `user_totp` row and every remaining recovery code.
    pub async fn disable(
        &self,
        user_id: UserId,
        email: &Email,
        submitted_totp: &str,
        ip: IpAddr,
        user_agent: &str,
    ) -> Result<(), AuthError> {
        let secret_cipher = self
            .repo
            .fetch_active_secret(user_id)
            .await?
            .ok_or(AuthError::TotpNotEnrolled)?;
        let secret = decrypt_fixed(&self.encryption_key, &secret_cipher)?;

        if !generator::verify_code(email, &secret, unix_now()?, submitted_totp)? {
            self.emit_audit(AuditEvent {
                kind: "auth.2fa.verify.failure",
                actor_user_id: Some(user_id),
                org_id: None,
                payload: {
                    let mut p = audit_context(ip, user_agent);
                    p["reason"] = json!("disable_wrong_code");
                    p
                },
            })
            .await;
            return Err(AuthError::TotpInvalidCode);
        }

        self.repo.delete_enrollment(user_id).await?;

        self.emit_audit(AuditEvent {
            kind: "auth.2fa.disable",
            actor_user_id: Some(user_id),
            org_id: None,
            payload: audit_context(ip, user_agent),
        })
        .await;

        Ok(())
    }

    /// Cheap enrollment check used by `/auth/me`. Returns `true` only
    /// for an active (confirmed, not disabled) enrollment.
    pub async fn is_enrolled(&self, user_id: UserId) -> Result<bool, AuthError> {
        self.repo.is_enrolled(user_id).await
    }

    async fn emit_audit(&self, event: AuditEvent) {
        if let Err(err) = self.audit.record(event).await {
            tracing::error!(error = %err, "failed to record totp audit event");
        }
    }
}

fn audit_context(ip: IpAddr, user_agent: &str) -> serde_json::Value {
    json!({
        "ip": crate::session::dto::mask_ip(ip),
        "ua": crate::session::dto::fingerprint_user_agent(user_agent),
    })
}

fn decrypt_fixed(
    key: &TotpEncryptionKey,
    cipher: &[u8],
) -> Result<crate::auth::totp::domain::TotpSecret, AuthError> {
    let arr: [u8; TOTP_SECRET_CIPHER_LEN] = cipher.try_into().map_err(|_| AuthError::TotpCrypto)?;
    crypto::decrypt_secret(key, &arr)
}

fn unix_now() -> Result<u64, AuthError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| AuthError::Internal(anyhow::anyhow!("system clock before unix epoch")))
}
