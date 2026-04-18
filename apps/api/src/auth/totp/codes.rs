//! Recovery-code generation, hashing, and verification.
//!
//! Each code is sampled uniformly from a confusable-free alphabet
//! ([`RECOVERY_CODE_ALPHABET`]), then hashed Argon2id + pepper — same
//! primitive as passwords, so rotating the pepper rotates both. The
//! plaintext is returned to the caller **once** (at enrollment success)
//! and never reconstructable server-side.

use aes_gcm::aead::rand_core::RngCore;
use rand::rngs::OsRng;
use secrecy::SecretString;

use crate::auth::{
    error::AuthError,
    hash::{hash_secret, verify_password},
    totp::domain::{
        RECOVERY_CODE_ALPHABET, RECOVERY_CODE_COUNT, RECOVERY_CODE_LEN, RecoveryCode,
        RecoveryCodeHash,
    },
};

/// Generate [`RECOVERY_CODE_COUNT`] codes. Returns `(plaintext_for_display,
/// argon2_hash_for_storage)` pairs — the caller shows the plaintext to
/// the user and persists the hashes.
///
/// # Errors
///
/// Propagates [`AuthError::Hashing`] if Argon2id fails — which in normal
/// operation never happens with the fixed parameters / valid pepper.
pub fn generate_recovery_codes(
    pepper: &SecretString,
) -> Result<Vec<(RecoveryCode, RecoveryCodeHash)>, AuthError> {
    let mut out = Vec::with_capacity(RECOVERY_CODE_COUNT);
    for _ in 0..RECOVERY_CODE_COUNT {
        let raw = random_code_raw();
        let hash = hash_secret(&raw, pepper)?;
        out.push((RecoveryCode(format_with_dash(&raw)), hash));
    }
    Ok(out)
}

/// Verify a submitted recovery code against the caller's **unused**
/// hashes. On match returns `Some(idx)` (position in the input slice)
/// so the caller marks that specific DB row as consumed; returns `None`
/// on any failure.
///
/// The submitted string is normalised (uppercased, dashes stripped) so
/// `"abcd-efgh"`, `"ABCD-EFGH"`, and `"ABCDEFGH"` all match identically.
///
/// # Errors
///
/// Propagates [`AuthError::Hashing`] if a stored hash is malformed —
/// distinct from a legitimate mismatch, which returns `Ok(None)`.
pub fn verify_recovery_code(
    submitted: &str,
    pepper: &SecretString,
    unused_hashes: &[RecoveryCodeHash],
) -> Result<Option<usize>, AuthError> {
    let canonical: String = submitted
        .chars()
        .filter(|c| *c != '-')
        .flat_map(char::to_uppercase)
        .collect();
    if canonical.len() != RECOVERY_CODE_LEN {
        return Ok(None);
    }
    for (i, h) in unused_hashes.iter().enumerate() {
        if verify_password(&canonical, pepper, h)? {
            return Ok(Some(i));
        }
    }
    Ok(None)
}

/// Sample a single code of [`RECOVERY_CODE_LEN`] chars using rejection
/// sampling over the alphabet. 30 alphabet chars means byte values
/// `0..240` (= 30 × 8) map uniformly; `240..256` are rejected to avoid
/// modulo bias.
fn random_code_raw() -> String {
    let alphabet_len = RECOVERY_CODE_ALPHABET.len() as u8;
    let bias_cutoff = (u8::MAX / alphabet_len) * alphabet_len;
    let mut out = String::with_capacity(RECOVERY_CODE_LEN);
    let mut byte = [0u8; 1];
    while out.len() < RECOVERY_CODE_LEN {
        OsRng.fill_bytes(&mut byte);
        if byte[0] >= bias_cutoff {
            continue;
        }
        let idx = (byte[0] % alphabet_len) as usize;
        out.push(RECOVERY_CODE_ALPHABET[idx] as char);
    }
    out
}

fn format_with_dash(raw: &str) -> String {
    let half = RECOVERY_CODE_LEN / 2;
    format!("{}-{}", &raw[..half], &raw[half..])
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    fn pepper() -> SecretString {
        SecretString::from("test-pepper-long-enough")
    }

    #[test]
    fn generate_produces_expected_shape() {
        let codes = generate_recovery_codes(&pepper()).expect("gen");
        assert_eq!(codes.len(), RECOVERY_CODE_COUNT);
        for (plain, _) in &codes {
            // XXXX-XXXX: RECOVERY_CODE_LEN chars + one dash.
            assert_eq!(plain.0.len(), RECOVERY_CODE_LEN + 1);
            assert_eq!(plain.0.chars().nth(RECOVERY_CODE_LEN / 2), Some('-'));
            for c in plain.0.chars().filter(|c| *c != '-') {
                assert!(
                    RECOVERY_CODE_ALPHABET.contains(&(c as u8)),
                    "char {c:?} not in confusable-free alphabet",
                );
            }
        }
    }

    #[test]
    fn generated_codes_are_distinct_within_a_batch() {
        let codes = generate_recovery_codes(&pepper()).expect("gen");
        let mut seen = std::collections::HashSet::new();
        for (plain, _) in &codes {
            assert!(seen.insert(plain.0.clone()), "duplicate code {}", plain.0);
        }
    }

    #[test]
    fn verify_matches_submitted_with_or_without_dash() {
        let codes = generate_recovery_codes(&pepper()).expect("gen");
        let (first_plain, _) = &codes[0];
        let hashes: Vec<_> = codes.iter().map(|(_, h)| h.clone()).collect();

        let with_dash = first_plain.0.clone();
        let no_dash = with_dash.replace('-', "");
        assert_eq!(
            verify_recovery_code(&with_dash, &pepper(), &hashes).expect("with dash"),
            Some(0),
        );
        assert_eq!(
            verify_recovery_code(&no_dash, &pepper(), &hashes).expect("no dash"),
            Some(0),
        );
    }

    #[test]
    fn verify_is_case_insensitive() {
        let codes = generate_recovery_codes(&pepper()).expect("gen");
        let (first_plain, _) = &codes[0];
        let hashes: Vec<_> = codes.iter().map(|(_, h)| h.clone()).collect();
        let lower = first_plain.0.to_lowercase();
        assert_eq!(
            verify_recovery_code(&lower, &pepper(), &hashes).expect("lower"),
            Some(0),
        );
    }

    #[test]
    fn verify_rejects_wrong_code() {
        let codes = generate_recovery_codes(&pepper()).expect("gen");
        let hashes: Vec<_> = codes.iter().map(|(_, h)| h.clone()).collect();
        assert_eq!(
            verify_recovery_code("AAAA-BBBB", &pepper(), &hashes).expect("wrong"),
            None,
        );
    }

    #[test]
    fn verify_rejects_wrong_length() {
        let codes = generate_recovery_codes(&pepper()).expect("gen");
        let hashes: Vec<_> = codes.iter().map(|(_, h)| h.clone()).collect();
        assert_eq!(
            verify_recovery_code("AAA-BBB", &pepper(), &hashes).expect("short"),
            None,
        );
        assert_eq!(
            verify_recovery_code("AAAAA-BBBBB", &pepper(), &hashes).expect("long"),
            None,
        );
    }

    #[test]
    fn verify_returns_none_after_hash_list_exhausted() {
        let codes = generate_recovery_codes(&pepper()).expect("gen");
        let (first_plain, _) = &codes[0];
        let empty: Vec<RecoveryCodeHash> = vec![];
        assert_eq!(
            verify_recovery_code(&first_plain.0, &pepper(), &empty).expect("empty"),
            None,
        );
    }
}
