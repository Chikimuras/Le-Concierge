//! Properties (biens) — first tenant-scoped domain.
//!
//! Layering per CLAUDE.md §7.1 and mirroring `auth/`:
//!
//! - [`domain`] — types + invariants (no IO).
//! - [`error`] — domain errors with an [`AppError`][crate::AppError] mapping.
//! - [`dto`] — request / response shapes (`utoipa` schemas).
//! - [`repo`] — SQLx persistence, every query scoped by `org_id`.
//! - [`service`] — orchestration + audit.
//! - [`routes`] — HTTP handlers under `/orgs/:slug/properties`.

pub mod domain;
pub mod dto;
pub mod error;
pub mod repo;
pub mod routes;
pub mod service;

pub use domain::{CreatePropertyInput, Property, PropertyId, UpdatePropertyInput};
pub use error::PropertyError;
pub use repo::PropertyRepo;
pub use service::PropertyService;
