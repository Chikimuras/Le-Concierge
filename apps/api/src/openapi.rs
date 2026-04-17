//! OpenAPI document and the routes that serve it.
//!
//! The schema is generated at compile time by [`utoipa`]; each domain
//! contributes its operations by exposing a `utoipa_axum::router::OpenApiRouter`
//! from its `routes::router()` function. The composition happens in
//! [`crate::app::build_app`], which then calls `.split_for_parts()` to obtain
//! the fully merged schema that this module serves at `/openapi.json`.
//!
//! The HTML UI is provided by [`utoipa_scalar`] (Scalar).
//!
//! # Security note
//!
//! Per `CLAUDE.md` §8, `/docs` must be gated behind admin authentication in
//! production. Phase 1 ships it publicly reachable and we will revisit
//! before any prod deploy (tracked as technical debt). The docs route
//! overrides the baseline CSP because Scalar loads its assets from a CDN;
//! hardening this to nonce-based CSP is also a prod-blocker TODO.

use axum::{
    Json, Router,
    http::{HeaderValue, header},
    response::IntoResponse,
    routing::get,
};
use tower_http::set_header::SetResponseHeaderLayer;
use utoipa::OpenApi;
use utoipa_scalar::{Scalar, Servable};

use crate::state::AppState;

/// Root OpenAPI document. Domain operations attach through the
/// `utoipa_axum::router::OpenApiRouter` used in `build_app`.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Le Concierge API",
        version = "0.1.0",
        description = "REST API for the Le Concierge SaaS concierge platform.",
        contact(name = "Le Concierge", email = "dev@leconcierge.local"),
        license(name = "Proprietary")
    ),
    tags(
        (name = "health", description = "Liveness and readiness probes.")
    )
)]
pub struct ApiDoc;

/// Router exposing the docs UI and the raw OpenAPI JSON.
///
/// Attaches its own (relaxed) CSP so Scalar's CDN-hosted assets load. This
/// replaces the baseline CSP from `crate::middleware::csp_layer` on routes
/// it handles.
pub fn docs_router(openapi: utoipa::openapi::OpenApi) -> Router<AppState> {
    let openapi_json = openapi.clone();
    let scalar_csp = HeaderValue::from_static(
        "default-src 'self'; \
         script-src 'self' https://cdn.jsdelivr.net 'unsafe-inline'; \
         style-src 'self' https://cdn.jsdelivr.net 'unsafe-inline'; \
         img-src 'self' data: https://cdn.jsdelivr.net; \
         font-src 'self' https://cdn.jsdelivr.net; \
         connect-src 'self';",
    );

    Router::new()
        .route(
            "/openapi.json",
            get(move || {
                let spec = openapi_json.clone();
                async move { Json(spec).into_response() }
            }),
        )
        .merge(Scalar::with_url("/docs", openapi))
        .layer(SetResponseHeaderLayer::overriding(
            header::CONTENT_SECURITY_POLICY,
            scalar_csp,
        ))
}
