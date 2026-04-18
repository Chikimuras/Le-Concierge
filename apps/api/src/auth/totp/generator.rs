//! Thin wrapper around `totp-rs` pinning our RFC-6238 parameters
//! (SHA1 / 6 digits / 30-s step / ±1 skew) and the issuer label.

use totp_rs::{Algorithm, TOTP};

use crate::auth::{
    domain::Email,
    error::AuthError,
    totp::domain::{TOTP_DIGITS, TOTP_ISSUER, TOTP_SKEW, TOTP_STEP_SECS, TotpSecret},
};

/// Build a `TOTP` instance bound to `(email, secret)`. The issuer and
/// account name are fixed by our threat model — no per-tenant rebranding
/// of the authenticator entry.
///
/// # Errors
///
/// Returns [`AuthError::TotpCrypto`] if `totp-rs` rejects the parameters,
/// which would only happen for an internally-broken secret length (20 is
/// always valid with SHA1).
pub fn totp_for(email: &Email, secret: &TotpSecret) -> Result<TOTP, AuthError> {
    TOTP::new(
        Algorithm::SHA1,
        TOTP_DIGITS,
        TOTP_SKEW,
        TOTP_STEP_SECS,
        secret.as_bytes().to_vec(),
        Some(TOTP_ISSUER.to_string()),
        email.as_str().to_string(),
    )
    .map_err(|_| AuthError::TotpCrypto)
}

/// `otpauth://totp/...` URL for the enrollment QR. Safe to hand back to
/// the client — the secret is base32-encoded inside it, but that is the
/// point (the user is about to scan it into their authenticator).
pub fn otpauth_url(email: &Email, secret: &TotpSecret) -> Result<String, AuthError> {
    Ok(totp_for(email, secret)?.get_url())
}

/// Generate the current 6-digit code at `unix_time`. Split out from
/// [`verify_code`] so tests can use a fixed clock to cover the skew
/// boundaries without racing `SystemTime::now`.
pub fn generate_code(
    email: &Email,
    secret: &TotpSecret,
    unix_time: u64,
) -> Result<String, AuthError> {
    Ok(totp_for(email, secret)?.generate(unix_time))
}

/// Verify a submitted 6-digit code at `unix_time`. Honors the configured
/// skew window (±1 step) internally; a `true` return means the code is
/// valid for the current, previous, or next 30-second slot.
pub fn verify_code(
    email: &Email,
    secret: &TotpSecret,
    unix_time: u64,
    submitted: &str,
) -> Result<bool, AuthError> {
    Ok(totp_for(email, secret)?.check(submitted, unix_time))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::auth::totp::crypto::generate_secret;

    fn email() -> Email {
        Email::parse("alice@example.test").expect("valid email")
    }

    #[test]
    fn generate_and_verify_round_trip_at_fixed_time() {
        let s = generate_secret();
        let t = 1_700_000_000_u64;
        let code = generate_code(&email(), &s, t).expect("gen");
        assert_eq!(code.len(), TOTP_DIGITS);
        assert!(verify_code(&email(), &s, t, &code).expect("verify"));
    }

    #[test]
    fn verify_accepts_previous_step_within_skew() {
        let s = generate_secret();
        let t_now = 1_700_000_030_u64; // on a step boundary
        let t_prev = t_now - TOTP_STEP_SECS;
        let code_prev = generate_code(&email(), &s, t_prev).expect("gen prev");
        assert!(verify_code(&email(), &s, t_now, &code_prev).expect("verify skew-"));
    }

    #[test]
    fn verify_accepts_next_step_within_skew() {
        let s = generate_secret();
        let t_now = 1_700_000_000_u64;
        let t_next = t_now + TOTP_STEP_SECS;
        let code_next = generate_code(&email(), &s, t_next).expect("gen next");
        assert!(verify_code(&email(), &s, t_now, &code_next).expect("verify skew+"));
    }

    #[test]
    fn verify_rejects_step_outside_skew() {
        let s = generate_secret();
        let t_now = 1_700_000_000_u64;
        let t_far = t_now + TOTP_STEP_SECS * 3;
        let code_far = generate_code(&email(), &s, t_far).expect("gen far");
        assert!(!verify_code(&email(), &s, t_now, &code_far).expect("verify"));
    }

    #[test]
    fn verify_rejects_wrong_code() {
        let s = generate_secret();
        assert!(!verify_code(&email(), &s, 1_700_000_000, "000000").expect("verify"));
    }

    #[test]
    fn otpauth_url_contains_issuer_and_email() {
        let s = generate_secret();
        let url = otpauth_url(&email(), &s).expect("url");
        assert!(url.starts_with("otpauth://totp/"));
        assert!(url.contains("Le%20Concierge"));
        assert!(url.contains("alice%40example.test"));
        assert!(url.contains("secret="));
    }
}
