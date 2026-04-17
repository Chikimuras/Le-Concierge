//! Liveness and readiness probes.
//!
//! Phase 1 ships only `/healthz` (liveness, process-level). `/readyz`, which
//! pings the database and other dependencies, lands alongside the first
//! migration in Phase 2.
//!
//! Per CLAUDE.md §7.1 this module follows the domain layout:
//! `dto.rs` for request/response shapes and `routes.rs` for handlers.

pub mod dto;
pub mod routes;
