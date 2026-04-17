//! Data transfer objects for the health module.
//!
//! Serializable with `serde` and schema-documented with `utoipa::ToSchema` so
//! the generated OpenAPI reflects the response shape.

use serde::Serialize;
use utoipa::ToSchema;

/// Liveness probe response. Returned by `GET /healthz`.
///
/// Always contains `status = "ok"` when the process is reachable. If the
/// process is degraded (e.g. dependency down), the future `/readyz` endpoint
/// is the right signal — liveness only indicates the binary is responding.
#[derive(Debug, Serialize, ToSchema)]
pub struct HealthStatus {
    /// Constant `"ok"`. Present to give a machine-readable field that won't
    /// change shape as the endpoint evolves.
    #[schema(example = "ok")]
    pub status: String,
    /// Crate version (`CARGO_PKG_VERSION`) baked in at compile time.
    #[schema(example = "0.1.0")]
    pub version: String,
    /// Service identifier, matches `telemetry.service_name` in logs.
    #[schema(example = "api")]
    pub service: String,
}

impl HealthStatus {
    /// Construct the canonical live response for this build.
    #[must_use]
    pub fn current() -> Self {
        Self {
            status: "ok".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            service: env!("CARGO_PKG_NAME").into(),
        }
    }
}
