//! Constants and newtypes for the TOTP module.

use crate::auth::domain::PasswordHash;

/// Raw TOTP shared-secret length in bytes. 20 bytes (160 bits) is the
/// RFC 6238 reference secret size and what every mainstream authenticator
/// app expects.
pub const TOTP_SECRET_LEN: usize = 20;

/// On-disk ciphertext length: 12-byte nonce || 20-byte secret || 16-byte
/// AES-GCM tag = 48 bytes. Checked at the DB level by `user_totp`'s
/// `octet_length(secret_cipher) = 48` constraint.
pub const TOTP_SECRET_CIPHER_LEN: usize = 48;

/// Number of digits returned to the user for a TOTP code.
pub const TOTP_DIGITS: usize = 6;

/// Step (period) in seconds. Every 30 s a new code is valid.
pub const TOTP_STEP_SECS: u64 = 30;

/// Verification window: accept codes one step before and after the current
/// one so clock drift of up to ±30 s does not lock users out.
pub const TOTP_SKEW: u8 = 1;

/// Issuer label rendered in the `otpauth://` URL and in authenticator app
/// listings. Kept short and stable — changing it later orphans existing
/// enrollments visually (the secret still works).
pub const TOTP_ISSUER: &str = "Le Concierge";

/// Number of recovery codes generated at enrollment success.
pub const RECOVERY_CODE_COUNT: usize = 10;

/// Character length of each recovery code before the dash is inserted
/// for display.
pub const RECOVERY_CODE_LEN: usize = 8;

/// Alphabet used to sample recovery codes. Base32-like but with the
/// confusable characters `0`/`O` and `1`/`I` removed, yielding 30 symbols
/// — roughly 4.91 bits per character, so 8 chars ≈ 39 bits of entropy.
pub const RECOVERY_CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ234567";

/// A raw 20-byte TOTP secret. Alive only during enrollment and
/// verification — never persisted in plaintext.
#[derive(Clone)]
pub struct TotpSecret(pub(crate) [u8; TOTP_SECRET_LEN]);

impl TotpSecret {
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; TOTP_SECRET_LEN] {
        &self.0
    }
}

impl std::fmt::Debug for TotpSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TotpSecret(REDACTED)")
    }
}

/// A freshly generated plaintext recovery code in the `XXXX-XXXX` display
/// form. Returned to the user **once** at enrollment success and never
/// reconstructed server-side.
#[derive(Debug, Clone)]
pub struct RecoveryCode(pub String);

/// Argon2id PHC string hashing one recovery code. Stored in
/// `user_totp_recovery_codes.code_hash`.
pub type RecoveryCodeHash = PasswordHash;
