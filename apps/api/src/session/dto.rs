//! Session identifier and stored payload.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{auth::UserId, session::error::SessionError};

/// Length in bytes of the raw session identifier before base64url encoding.
const SID_BYTES: usize = 32;
/// Length of the encoded string form (no padding).
const SID_ENCODED_LEN: usize = 43; // ceil(SID_BYTES * 8 / 6)

/// Opaque session identifier. 32 bytes of CSPRNG entropy, base64url-encoded
/// without padding. The encoded form goes in the `lc_sid` cookie and in
/// the Redis key; the raw bytes never leave the generator.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionId(String);

impl SessionId {
    /// Generate a fresh identifier using the OS CSPRNG.
    #[must_use]
    pub fn generate() -> Self {
        let mut buf = [0u8; SID_BYTES];
        OsRng.fill_bytes(&mut buf);
        Self(URL_SAFE_NO_PAD.encode(buf))
    }

    /// Parse a user-supplied string as a session identifier. Rejects
    /// anything that does not match the canonical shape without ever
    /// leaking *why* the string is bad (the answer is always "unauthenticated").
    pub fn parse(raw: &str) -> Result<Self, SessionError> {
        if raw.len() != SID_ENCODED_LEN {
            return Err(SessionError::Malformed);
        }
        URL_SAFE_NO_PAD
            .decode(raw)
            .map_err(|_| SessionError::Malformed)?;
        Ok(Self(raw.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Redis key under which the session payload is stored.
    #[must_use]
    pub fn redis_key(&self) -> String {
        format!("session:{}", self.0)
    }
}

/// Generate a CSRF token (same shape as `SessionId` — 32 random bytes
/// encoded as 43-char base64url). Stored inside `SessionData` and echoed
/// by the client in the `X-CSRF-Token` header.
#[must_use]
pub fn generate_csrf_token() -> String {
    let mut buf = [0u8; SID_BYTES];
    OsRng.fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

/// Payload stored in Redis against a [`SessionId`]. Never reaches clients
/// verbatim — they get a sanitized [`SessionMeta`] instead.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    pub user_id: UserId,
    /// Double-submit CSRF secret. See [`SessionData::csrf_matches`].
    pub csrf_token: String,
    /// Whether the user has completed the second factor this session.
    /// Phase 4c promotes this flag on successful TOTP verification.
    pub mfa_verified: bool,
    pub created_at: DateTime<Utc>,
    /// Hard cut-off: the session is destroyed once `Utc::now()` passes
    /// this timestamp, regardless of activity.
    pub absolute_expires_at: DateTime<Utc>,
    /// IP address seen at session creation, masked to the /24 for
    /// IPv4 / /48 for IPv6 before storage (CLAUDE.md §3.3).
    pub ip_masked: String,
    /// SHA-256(User-Agent) first 16 bytes hex-encoded. Lets us detect
    /// client changes without storing the full UA string.
    pub user_agent_fingerprint: String,
}

impl SessionData {
    /// Constant-time comparison of the stored CSRF token with a header
    /// value (OWASP ASVS 4.1.2, timing-attack resistant).
    #[must_use]
    pub fn csrf_matches(&self, provided: &str) -> bool {
        // `constant_time_eq` would be nicer; for 43-char tokens this is
        // effectively constant-time when the two are the same length.
        // We still mask length early to avoid leaking length info.
        let a = self.csrf_token.as_bytes();
        let b = provided.as_bytes();
        if a.len() != b.len() {
            return false;
        }
        let mut diff: u8 = 0;
        for (x, y) in a.iter().zip(b.iter()) {
            diff |= x ^ y;
        }
        diff == 0
    }

    /// Returns `true` if `now` is past the absolute cut-off.
    #[must_use]
    pub fn is_absolutely_expired(&self, now: DateTime<Utc>) -> bool {
        now >= self.absolute_expires_at
    }
}

/// Public projection of a session exposed to the client (never includes
/// Redis-internal details, IPs, or secrets).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SessionMeta {
    pub user_id: UserId,
    pub csrf_token: String,
    pub mfa_verified: bool,
    pub created_at: DateTime<Utc>,
    pub absolute_expires_at: DateTime<Utc>,
}

impl From<&SessionData> for SessionMeta {
    fn from(data: &SessionData) -> Self {
        Self {
            user_id: data.user_id,
            csrf_token: data.csrf_token.clone(),
            mfa_verified: data.mfa_verified,
            created_at: data.created_at,
            absolute_expires_at: data.absolute_expires_at,
        }
    }
}

/// Mask an IP address for storage: drop the last octet (IPv4) or the
/// last 80 bits (IPv6). Keeps enough signal for audit / security review
/// without leaking precise location data.
#[must_use]
pub fn mask_ip(ip: std::net::IpAddr) -> String {
    match ip {
        std::net::IpAddr::V4(v4) => {
            let o = v4.octets();
            format!("{}.{}.{}.0", o[0], o[1], o[2])
        }
        std::net::IpAddr::V6(v6) => {
            let seg = v6.segments();
            format!("{:x}:{:x}:{:x}::", seg[0], seg[1], seg[2])
        }
    }
}

/// Stable, short fingerprint of a User-Agent string. Hex-encoded first
/// 16 bytes of SHA-256.
#[must_use]
pub fn fingerprint_user_agent(ua: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(ua.as_bytes());
    hex_short(&digest[..8])
}

fn hex_short(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn session_id_round_trips_through_parse() {
        let id = SessionId::generate();
        assert_eq!(id.as_str().len(), SID_ENCODED_LEN);
        let parsed = SessionId::parse(id.as_str()).expect("valid");
        assert_eq!(parsed, id);
    }

    #[test]
    fn session_id_rejects_wrong_length() {
        assert!(matches!(
            SessionId::parse("short"),
            Err(SessionError::Malformed)
        ));
        assert!(matches!(
            SessionId::parse(&"a".repeat(44)),
            Err(SessionError::Malformed)
        ));
    }

    #[test]
    fn session_id_rejects_bad_charset() {
        // Forty-three-char string but with a non-base64url character.
        let bad = format!("{}!", "a".repeat(42));
        assert!(matches!(
            SessionId::parse(&bad),
            Err(SessionError::Malformed)
        ));
    }

    #[test]
    fn csrf_matches_is_length_safe() {
        let data = SessionData {
            user_id: UserId::new(),
            csrf_token: "secret-token".into(),
            mfa_verified: false,
            created_at: Utc::now(),
            absolute_expires_at: Utc::now(),
            ip_masked: "127.0.0.0".into(),
            user_agent_fingerprint: "abc".into(),
        };
        assert!(data.csrf_matches("secret-token"));
        assert!(!data.csrf_matches("secret-tokeN"));
        assert!(!data.csrf_matches("secret-tokenXX"));
        assert!(!data.csrf_matches(""));
    }

    #[test]
    fn mask_ip_removes_last_octet_ipv4() {
        let ip: std::net::IpAddr = "192.168.1.42".parse().unwrap();
        assert_eq!(mask_ip(ip), "192.168.1.0");
    }

    #[test]
    fn fingerprint_is_stable_and_short() {
        let a = fingerprint_user_agent("Mozilla/5.0");
        let b = fingerprint_user_agent("Mozilla/5.0");
        assert_eq!(a, b);
        assert_eq!(a.len(), 16);
    }
}
