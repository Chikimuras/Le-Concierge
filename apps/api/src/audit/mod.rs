//! Immutable, hash-chained audit log.
//!
//! Every security-relevant action (signup, login, logout, role change,
//! payment, …) lands in `audit_events` through [`record_event`]. The row
//! is part of a SHA-256 chain (`hash = SHA-256(prev_hash || canonical)`),
//! so tampering with any record past or present invalidates subsequent
//! entries.
//!
//! Concurrency: the insert runs inside a caller-provided SQL transaction
//! and takes a fixed Postgres advisory lock, so two concurrent writers
//! cannot interleave and break the chain.
//!
//! See CLAUDE.md §3.3, ADR 0002 (security baseline), and ADR 0006
//! (sessions & CSRF) for the surrounding policy.

pub mod record;

pub use record::{AuditEvent, AuditRepo};
