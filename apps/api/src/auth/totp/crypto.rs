//! AES-256-GCM wrapping of the TOTP shared secret.
//!
//! Key material comes from `APP_AUTH__TOTP_KEY` (hex-encoded, 64 chars →
//! 32 bytes). Kept in a [`TotpEncryptionKey`] for the lifetime of the
//! process; a future rotation strategy adds a `key_version` column and a
//! background re-wrap job (ADR 0007).

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, rand_core::RngCore},
};
use rand::rngs::OsRng;
use secrecy::{ExposeSecret, SecretString};

use crate::auth::{
    error::AuthError,
    totp::domain::{TOTP_SECRET_CIPHER_LEN, TOTP_SECRET_LEN, TotpSecret},
};

const NONCE_LEN: usize = 12;

/// Parsed 32-byte AES-256 key, loaded once at boot.
#[derive(Clone)]
pub struct TotpEncryptionKey([u8; 32]);

impl TotpEncryptionKey {
    /// Parse a 64-char hex string into a 32-byte key.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::TotpKeyInvalid`] if the input is not valid
    /// hex or does not decode to exactly 32 bytes. Fail-closed at boot
    /// matches the pepper posture (ADR 0005).
    pub fn from_hex(hex_str: &SecretString) -> Result<Self, AuthError> {
        let raw =
            hex::decode(hex_str.expose_secret().trim()).map_err(|_| AuthError::TotpKeyInvalid)?;
        let key: [u8; 32] = raw.try_into().map_err(|_| AuthError::TotpKeyInvalid)?;
        Ok(Self(key))
    }
}

impl std::fmt::Debug for TotpEncryptionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TotpEncryptionKey(REDACTED)")
    }
}

/// Encrypt a TOTP secret with AES-256-GCM. Output layout:
/// `nonce (12) || ciphertext (20) || tag (16)` = 48 bytes. The nonce is
/// fresh from `OsRng` per call — never reused with the same key.
///
/// # Errors
///
/// Returns [`AuthError::TotpCrypto`] if the underlying AEAD operation
/// fails. In practice this only happens for memory-allocation issues
/// since the key / nonce / plaintext sizes are all fixed and valid.
pub fn encrypt_secret(
    key: &TotpEncryptionKey,
    secret: &TotpSecret,
) -> Result<[u8; TOTP_SECRET_CIPHER_LEN], AuthError> {
    let cipher = Aes256Gcm::new(key.0.as_slice().into());
    let mut nonce = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce);
    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce), secret.0.as_slice())
        .map_err(|_| AuthError::TotpCrypto)?;

    let mut out = [0u8; TOTP_SECRET_CIPHER_LEN];
    out[..NONCE_LEN].copy_from_slice(&nonce);
    out[NONCE_LEN..].copy_from_slice(&ct);
    Ok(out)
}

/// Decrypt a stored TOTP ciphertext back into the 20-byte secret. A
/// tampered nonce, ciphertext, or tag all fail the AEAD check and return
/// [`AuthError::TotpCrypto`] — the caller never learns which byte was
/// flipped.
pub fn decrypt_secret(
    key: &TotpEncryptionKey,
    cipher_bytes: &[u8; TOTP_SECRET_CIPHER_LEN],
) -> Result<TotpSecret, AuthError> {
    let (nonce_bytes, ct_tag) = cipher_bytes.split_at(NONCE_LEN);
    let cipher = Aes256Gcm::new(key.0.as_slice().into());
    let pt = cipher
        .decrypt(Nonce::from_slice(nonce_bytes), ct_tag)
        .map_err(|_| AuthError::TotpCrypto)?;
    let secret: [u8; TOTP_SECRET_LEN] = pt.try_into().map_err(|_| AuthError::TotpCrypto)?;
    Ok(TotpSecret(secret))
}

/// Generate a fresh 20-byte TOTP secret from `OsRng`.
#[must_use]
pub fn generate_secret() -> TotpSecret {
    let mut bytes = [0u8; TOTP_SECRET_LEN];
    OsRng.fill_bytes(&mut bytes);
    TotpSecret(bytes)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    fn key() -> TotpEncryptionKey {
        TotpEncryptionKey::from_hex(&SecretString::from(
            "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        ))
        .expect("valid 64-char hex")
    }

    #[test]
    fn encrypt_then_decrypt_round_trips() {
        let k = key();
        let s = generate_secret();
        let c = encrypt_secret(&k, &s).expect("encrypt");
        let back = decrypt_secret(&k, &c).expect("decrypt");
        assert_eq!(s.as_bytes(), back.as_bytes());
    }

    #[test]
    fn each_encrypt_produces_a_distinct_nonce() {
        let k = key();
        let s = generate_secret();
        let a = encrypt_secret(&k, &s).expect("a");
        let b = encrypt_secret(&k, &s).expect("b");
        assert_ne!(a, b, "nonce reuse would defeat AES-GCM");
    }

    #[test]
    fn tampered_tag_fails_verification() {
        let k = key();
        let s = generate_secret();
        let mut c = encrypt_secret(&k, &s).expect("encrypt");
        // Flip a bit inside the 16-byte tag (last 16 bytes).
        c[TOTP_SECRET_CIPHER_LEN - 1] ^= 0x01;
        let err = decrypt_secret(&k, &c).unwrap_err();
        assert!(matches!(err, AuthError::TotpCrypto));
    }

    #[test]
    fn tampered_ciphertext_fails_verification() {
        let k = key();
        let s = generate_secret();
        let mut c = encrypt_secret(&k, &s).expect("encrypt");
        // Flip a bit in the ciphertext region (after nonce, before tag).
        c[NONCE_LEN + 5] ^= 0x01;
        let err = decrypt_secret(&k, &c).unwrap_err();
        assert!(matches!(err, AuthError::TotpCrypto));
    }

    #[test]
    fn wrong_key_fails_verification() {
        let k1 = key();
        let k2 = TotpEncryptionKey::from_hex(&SecretString::from(
            "ffeeddccbbaa99887766554433221100ffeeddccbbaa99887766554433221100",
        ))
        .expect("valid");
        let s = generate_secret();
        let c = encrypt_secret(&k1, &s).expect("encrypt");
        let err = decrypt_secret(&k2, &c).unwrap_err();
        assert!(matches!(err, AuthError::TotpCrypto));
    }

    #[test]
    fn hex_parsing_rejects_wrong_length() {
        let err = TotpEncryptionKey::from_hex(&SecretString::from("00112233")).unwrap_err();
        assert!(matches!(err, AuthError::TotpKeyInvalid));
    }

    #[test]
    fn hex_parsing_rejects_non_hex() {
        let err = TotpEncryptionKey::from_hex(&SecretString::from(
            "ZZ112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        ))
        .unwrap_err();
        assert!(matches!(err, AuthError::TotpKeyInvalid));
    }
}
