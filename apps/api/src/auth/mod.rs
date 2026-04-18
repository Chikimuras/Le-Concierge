//! Authentication domain.
//!
//! Layered as per `CLAUDE.md` §7.1:
//!
//! - [`domain`] — pure types (no IO), newtypes that enforce invariants.
//! - [`error`] — domain errors with a clean [`AppError`][crate::AppError]
//!   conversion.
//! - [`hash`] — Argon2id + pepper primitives.
//! - [`repo`] — SQLx persistence gateway.
//! - [`service`] — orchestration used by HTTP handlers (land in Phase 4b).
//!
//! Security references: ADR 0005 and OWASP ASVS §2 (authentication), §3
//! (session management).

pub mod domain;
pub mod dto;
pub mod error;
pub mod hash;
pub mod repo;
pub mod routes;
pub mod service;
pub mod totp;

pub use domain::{Email, OrgId, PasswordHash, Role, Slug, UserId};
pub use error::AuthError;
pub use repo::AuthRepo;
pub use service::{AuthService, LoginInput, SessionIssue, SignupInput, SignupOutcome, UserContext};
