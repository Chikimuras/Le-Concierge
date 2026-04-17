//! Health probe handlers.

use axum::Json;
use utoipa_axum::{router::OpenApiRouter, routes};

use super::dto::HealthStatus;
use crate::state::AppState;

/// Liveness probe. Returns 200 as long as the process is reachable.
///
/// This handler is intentionally stateless: it does not touch the database,
/// Redis, or any other dependency. `/readyz` (Phase 2) is the endpoint that
/// reflects dependency health.
#[utoipa::path(
    get,
    path = "/healthz",
    tag = "health",
    responses(
        (status = 200, description = "Service process is alive", body = HealthStatus)
    )
)]
pub async fn healthz() -> Json<HealthStatus> {
    Json(HealthStatus::current())
}

/// Router exposing the health endpoints plus their OpenAPI definitions.
#[must_use]
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(healthz))
}
