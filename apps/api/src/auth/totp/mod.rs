//! TOTP 2FA primitives and orchestration.
//!
//! Parameters (ADR 0007): SHA1, 6 digits, 30-second step, ±1 step skew
//! window, 20-byte `OsRng` secret wrapped at rest with AES-256-GCM via a
//! dedicated key (`APP_AUTH__TOTP_KEY`). Recovery codes are 10 × 8-char
//! samples from a confusable-free alphabet, hashed Argon2id+pepper.
//!
//! References: RFC 6238 (TOTP); OWASP ASVS §2.7 (MFA); CLAUDE.md §3.1.

pub mod codes;
pub mod crypto;
pub mod domain;
pub mod dto;
pub mod generator;
pub mod repo;
pub mod routes;
pub mod service;

pub use crypto::TotpEncryptionKey;
pub use domain::{
    RECOVERY_CODE_ALPHABET, RECOVERY_CODE_COUNT, RECOVERY_CODE_LEN, RecoveryCode, RecoveryCodeHash,
    TOTP_DIGITS, TOTP_ISSUER, TOTP_SECRET_CIPHER_LEN, TOTP_SECRET_LEN, TOTP_SKEW, TOTP_STEP_SECS,
    TotpSecret,
};
pub use repo::{RecoveryCodeRow, TotpRepo, TotpRow};
pub use service::{TotpEnrollmentStart, TotpService, TotpVerifyOutcome};
