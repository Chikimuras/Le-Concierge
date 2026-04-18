//! Team invites (Phase 5b).
//!
//! Design: ADR 0009. Mirrors the `auth/` layout — pure types +
//! errors + DTOs + SQLx repo + service + HTTP handlers.

pub mod domain;
pub mod dto;
pub mod error;
pub mod repo;
pub mod routes;
pub mod service;

pub use domain::{Invite, InviteId};
pub use error::InviteError;
pub use repo::InviteRepo;
pub use service::{AcceptOutcome, InviteService};
