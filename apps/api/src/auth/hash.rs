//! Password hashing (Argon2id + pepper).
//!
//! Parameters follow OWASP's 2024+ recommendation for Argon2id:
//!
//! - memory cost `m = 19 456 KiB` (~19 MiB)
//! - time cost `t = 2`
//! - parallelism `p = 1`
//!
//! Ref: <https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html#argon2id>
//!
//! The **pepper** is an application-wide secret injected into every hash.
//! It lives only in `APP_AUTH__PEPPER` (env var / Docker secret) — never in
//! the database or in a TOML file (CLAUDE.md §3.1). Rotating the pepper
//! requires re-hashing every password; we will handle that in a later
//! migration when (if) the need arises.

use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use secrecy::{ExposeSecret, SecretString};

use crate::auth::{domain::PasswordHash, error::AuthError};

// Parameters are compile-time constants so `cargo deny` / future audits
// can diff them easily.
const MEMORY_KIB: u32 = 19_456;
const TIME_COST: u32 = 2;
const PARALLELISM: u32 = 1;

/// Minimum length for new passwords. NIST SP 800-63B §5.1.1.2 — length is
/// the most effective password complexity lever. Richer policy (breached
/// password check via k-anonymity, zxcvbn scoring, …) arrives in Phase 4b.
pub const MIN_PASSWORD_LEN: usize = 12;

fn hasher(pepper: &SecretString) -> Result<Argon2<'_>, AuthError> {
    // Compile-time constants are valid, but `Params::new` and
    // `Argon2::new_with_secret` still return `Result` on edge cases (e.g.
    // a pepper exceeding the internal secret-length cap). Both surface as
    // `AuthError::HashingConfig` — an operator/config error distinct from
    // `Hashing` (which covers runtime failures on individual passwords).
    let params = Params::new(MEMORY_KIB, TIME_COST, PARALLELISM, None)
        .map_err(|_| AuthError::HashingConfig("argon2-params"))?;
    Argon2::new_with_secret(
        pepper.expose_secret().as_bytes(),
        Algorithm::Argon2id,
        Version::V0x13,
        params,
    )
    .map_err(|_| AuthError::HashingConfig("argon2-secret"))
}

/// Hash a plaintext password with Argon2id + pepper.
///
/// # Errors
///
/// Returns [`AuthError::WeakPassword`] if the password is shorter than
/// [`MIN_PASSWORD_LEN`], or [`AuthError::Hashing`] if Argon2id itself fails
/// (e.g. the pepper is too long for the chosen parameters — should never
/// happen in normal operation).
pub fn hash_password(plain: &str, pepper: &SecretString) -> Result<PasswordHash, AuthError> {
    if plain.len() < MIN_PASSWORD_LEN {
        return Err(AuthError::WeakPassword);
    }
    hash_secret(plain, pepper)
}

/// Hash an arbitrary high-entropy secret with Argon2id + pepper. Skips
/// the [`MIN_PASSWORD_LEN`] guard — the caller is responsible for the
/// entropy budget. Used for recovery codes (8 chars, ~39 bits, sampled
/// from a CSPRNG) where the length policy does not apply.
///
/// # Errors
///
/// Returns [`AuthError::Hashing`] on an underlying Argon2id failure —
/// in normal operation, only a misconfigured pepper triggers this.
pub fn hash_secret(plain: &str, pepper: &SecretString) -> Result<PasswordHash, AuthError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon = hasher(pepper)?;
    let phc = argon
        .hash_password(plain.as_bytes(), &salt)
        .map_err(AuthError::Hashing)?
        .to_string();
    Ok(PasswordHash::new_unchecked(phc))
}

/// Verify a plaintext password against a stored PHC hash.
///
/// Returns `Ok(true)` on match, `Ok(false)` on mismatch, and
/// [`AuthError::Hashing`] on a malformed stored hash. Callers decide how
/// to present the result — typically as [`AuthError::InvalidCredentials`]
/// to the user (never reveal mismatch vs. malformed).
pub fn verify_password(
    plain: &str,
    pepper: &SecretString,
    stored: &PasswordHash,
) -> Result<bool, AuthError> {
    let parsed =
        argon2::password_hash::PasswordHash::new(stored.as_db_str()).map_err(AuthError::Hashing)?;
    let argon = hasher(pepper)?;
    match argon.verify_password(plain.as_bytes(), &parsed) {
        Ok(()) => Ok(true),
        Err(argon2::password_hash::Error::Password) => Ok(false),
        Err(other) => Err(AuthError::Hashing(other)),
    }
}

/// Produce a valid Argon2id PHC hash bound to `pepper` for use as a
/// timing-equalization target in the login flow (OWASP ASVS 2.1.1).
///
/// Computed once at boot, this hash cannot match any password an attacker
/// can choose: the plaintext is derived from a per-boot `OsRng` salt and
/// never surfaces. Verifying an arbitrary password against it performs
/// the full Argon2id work, letting us make the "unknown email" and
/// "account locked" branches of login indistinguishable by latency from
/// the "wrong password" branch.
pub fn precompute_dummy_hash(pepper: &SecretString) -> Result<PasswordHash, AuthError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon = hasher(pepper)?;
    // The salt bytes double as the throwaway plaintext: `OsRng`-sourced
    // entropy that no attacker can reproduce to make `verify_password`
    // return `Ok(true)`.
    let phc = argon
        .hash_password(salt.as_str().as_bytes(), &salt)
        .map_err(AuthError::Hashing)?
        .to_string();
    Ok(PasswordHash::new_unchecked(phc))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use secrecy::SecretString;

    use super::*;

    fn pepper() -> SecretString {
        SecretString::from("test-pepper-long-enough")
    }

    #[test]
    fn round_trip_matches() {
        let password = "correct-horse-battery-staple";
        let hash = hash_password(password, &pepper()).expect("hash ok");
        assert!(verify_password(password, &pepper(), &hash).expect("verify ok"));
    }

    #[test]
    fn wrong_password_fails() {
        let hash = hash_password("correct-horse-battery-staple", &pepper()).expect("hash");
        assert!(!verify_password("wrong-password", &pepper(), &hash).expect("verify"));
    }

    #[test]
    fn different_pepper_fails_verification() {
        let hash = hash_password("correct-horse-battery-staple", &pepper()).expect("hash");
        let other_pepper = SecretString::from("another-pepper-entirely");
        assert!(
            !verify_password("correct-horse-battery-staple", &other_pepper, &hash).expect("verify")
        );
    }

    #[test]
    fn short_password_is_rejected() {
        let too_short = "abc123";
        let err = hash_password(too_short, &pepper()).unwrap_err();
        assert!(matches!(err, AuthError::WeakPassword));
    }

    #[test]
    fn malformed_hash_is_hashing_error() {
        let bogus = PasswordHash::new_unchecked("not a phc string".into());
        let err = verify_password("whatever", &pepper(), &bogus).unwrap_err();
        assert!(matches!(err, AuthError::Hashing(_)));
    }

    #[test]
    fn dummy_hash_is_valid_and_never_matches_foreign_input() {
        let dummy = precompute_dummy_hash(&pepper()).expect("dummy ok");
        // Real PHC string: parses successfully.
        argon2::password_hash::PasswordHash::new(dummy.as_db_str()).expect("parse phc");
        // Cannot be matched by any caller-supplied password — the full
        // Argon2id work runs, then returns Ok(false).
        assert!(!verify_password("whatever", &pepper(), &dummy).expect("verify"));
    }
}
