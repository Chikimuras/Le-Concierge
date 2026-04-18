//! Server-side session management.
//!
//! Layered per CLAUDE.md §7.1:
//!
//! - [`dto`] — session ID newtype, serialisable session data.
//! - [`store`] — Redis persistence.
//! - [`service`] — the [`SessionService`] handlers consume.
//! - [`cookie`] — helpers to build `lc_sid` cookies.
//! - [`extractor`] — the [`AuthenticatedUser`] Axum extractor.
//! - [`csrf`] — double-submit CSRF middleware.
//!
//! See ADR 0006 for the design (format, TTLs, lockout, rate-limit).

pub mod cookie;
pub mod csrf;
pub mod dto;
pub mod error;
pub mod extractor;
pub mod service;
pub mod store;

pub use cookie::{SESSION_COOKIE_NAME, clear_session_cookie, session_cookie};
pub use dto::{SessionData, SessionId, SessionMeta};
pub use error::SessionError;
pub use extractor::AuthenticatedUser;
pub use service::SessionService;
pub use store::RedisSessionStore;
