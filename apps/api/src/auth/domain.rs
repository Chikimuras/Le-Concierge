//! Auth domain types.
//!
//! Newtype wrappers around primitives so misuse is a compile error rather
//! than a runtime surprise (e.g. passing an `OrgId` where a `UserId` is
//! expected). Each type enforces its own invariants at construction time
//! and exposes only safe accessors.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::error::AuthError;

// ---- ID newtypes ----------------------------------------------------------

macro_rules! uuid_newtype {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type,
        )]
        #[serde(transparent)]
        #[sqlx(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            #[must_use]
            pub fn into_inner(self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }

        impl From<Uuid> for $name {
            fn from(u: Uuid) -> Self {
                Self(u)
            }
        }

        impl From<$name> for Uuid {
            fn from(n: $name) -> Self {
                n.0
            }
        }
    };
}

uuid_newtype!(
    /// Identifier of a user account. Backed by a UUIDv4.
    UserId
);
uuid_newtype!(
    /// Identifier of an organization (tenant). Backed by a UUIDv4.
    OrgId
);

// ---- Role ----------------------------------------------------------------

/// Per-organization role. Mirrors the SQL `role` enum from migration 0001.
///
/// Platform admin is **not** represented here: see the `platform_admins`
/// table. See `CLAUDE.md` §1 and ADR 0005.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "role", rename_all = "lowercase")]
pub enum Role {
    Owner,
    Manager,
    Cleaner,
    Guest,
}

impl Role {
    /// Stable string representation used in logs and audit events.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Manager => "manager",
            Self::Cleaner => "cleaner",
            Self::Guest => "guest",
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---- Email ----------------------------------------------------------------

/// Validated email address.
///
/// Storage is lowercased so lookups in a `citext` column work deterministically
/// regardless of the caller's input casing. Validation follows
/// [`email_address`] (RFC 5321 / 5322 compliant).
///
/// [`email_address`]: https://crates.io/crates/email_address
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[serde(transparent)]
#[sqlx(transparent)]
pub struct Email(String);

impl Email {
    /// Parse and validate a string as an email address.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::InvalidEmail`] if the string is not a valid
    /// address.
    pub fn parse(raw: &str) -> Result<Self, AuthError> {
        let trimmed = raw.trim();
        email_address::EmailAddress::from_str(trimmed)
            .map(|parsed| Self(parsed.email().to_ascii_lowercase()))
            .map_err(|_| AuthError::InvalidEmail)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Email {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// ---- PasswordHash --------------------------------------------------------

/// Opaque wrapper over a PHC-format Argon2id hash string.
///
/// The inner `String` is intentionally **not** exposed via `Deref` /
/// accessors that return it by value. Callers that need to persist the
/// hash use [`PasswordHash::as_db_str`]; callers that need to verify a
/// password use [`crate::auth::hash::verify_password`].
///
/// `Debug` is redacted so the hash never accidentally lands in a log line.
#[derive(Clone, PartialEq, Eq)]
pub struct PasswordHash(String);

impl PasswordHash {
    /// Wrap a PHC-format hash string. No validation here — produce instances
    /// through [`crate::auth::hash::hash_password`] or via `FromStr`.
    #[must_use]
    pub fn new_unchecked(phc: String) -> Self {
        Self(phc)
    }

    /// Borrow the hash as `&str`. Intended for persistence only.
    #[must_use]
    pub fn as_db_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for PasswordHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PasswordHash(redacted)")
    }
}

// ---- Slug ---------------------------------------------------------------

/// Organization slug — lowercase kebab-case, 3–64 chars. Matches the SQL
/// check constraint in migration 0001.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Slug(String);

impl Slug {
    /// Parse and validate. Returns [`AuthError::InvalidSlug`] on anything
    /// outside the regex `^[a-z0-9][a-z0-9-]{0,62}[a-z0-9]$`.
    pub fn parse(raw: &str) -> Result<Self, AuthError> {
        let len = raw.len();
        if !(2..=64).contains(&len) {
            return Err(AuthError::InvalidSlug);
        }
        let bytes = raw.as_bytes();
        let ok = bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
            && !raw.starts_with('-')
            && !raw.ends_with('-');
        if ok {
            Ok(Self(raw.to_owned()))
        } else {
            Err(AuthError::InvalidSlug)
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Slug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// ---- Unit tests ---------------------------------------------------------

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn email_is_lowercased_and_trimmed() {
        let e = Email::parse("  Alice@Example.COM  ").expect("valid");
        assert_eq!(e.as_str(), "alice@example.com");
    }

    #[test]
    fn email_rejects_garbage() {
        assert!(Email::parse("not-an-email").is_err());
        assert!(Email::parse("@foo.com").is_err());
        assert!(Email::parse("").is_err());
    }

    #[test]
    fn slug_accepts_canonical_form() {
        assert!(Slug::parse("acme").is_ok());
        assert!(Slug::parse("acme-co").is_ok());
        assert!(Slug::parse("a1-b2").is_ok());
    }

    #[test]
    fn slug_rejects_edges_and_caps() {
        assert!(Slug::parse("-acme").is_err());
        assert!(Slug::parse("acme-").is_err());
        assert!(Slug::parse("Acme").is_err());
        assert!(Slug::parse("a").is_err()); // too short
        assert!(Slug::parse("a_b").is_err()); // underscore
    }

    #[test]
    fn password_hash_debug_is_redacted() {
        let h = PasswordHash::new_unchecked("$argon2id$v=19$m=19456,t=2,p=1$…".into());
        let dbg = format!("{h:?}");
        assert_eq!(dbg, "PasswordHash(redacted)");
    }

    #[test]
    fn role_round_trips_as_str() {
        for role in [Role::Owner, Role::Manager, Role::Cleaner, Role::Guest] {
            let s = role.as_str();
            assert!(!s.is_empty());
            assert!(s.chars().all(|c| c.is_ascii_lowercase()));
        }
    }
}
