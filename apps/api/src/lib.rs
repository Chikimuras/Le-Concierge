//! Library facade over the `api` binary.
//!
//! Exposed so that integration tests under `tests/` can spawn a real instance
//! of the application without duplicating bootstrap code. Prefer using
//! [`app::build_app`] from tests rather than running `main.rs` directly.

pub mod app;
pub mod auth;
pub mod config;
pub mod error;
pub mod health;
pub mod middleware;
pub mod openapi;
pub mod state;
pub mod telemetry;

pub use app::build_app;
pub use config::Config;
pub use error::AppError;
pub use state::AppState;
