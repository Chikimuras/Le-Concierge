//! Compose the Axum router: domain routes, OpenAPI docs, and middleware.
//!
//! Middleware stack (inner → outer, i.e. order of execution on the response
//! path):
//!
//! 1. Route-local layers (Scalar CSP, future auth, etc.)
//! 2. Security response headers (HSTS, CSP, nosniff, frame-deny, referrer,
//!    permissions, COOP, CORP)
//! 3. CORS
//! 4. Compression
//! 5. Request timeout
//! 6. `x-request-id` propagation + `traceparent` propagation
//! 7. Sensitive response headers redaction for tracing
//! 8. HTTP request tracing span
//! 9. Sensitive request headers redaction for tracing
//! 10. `x-request-id` generation
//! 11. Panic catch (outermost)
//!
//! Request flow is the reverse: panic catch first, handler last. See
//! `CLAUDE.md` §3.2 / §5 and ADR 0002.

use axum::Router;
use tower::ServiceBuilder;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;

use crate::{health, middleware as mw, openapi::ApiDoc, state::AppState};

/// Build the fully composed application router.
///
/// Returns a stateless [`Router`] — the incoming [`AppState`] is baked into
/// every handler via `with_state`, so the caller is left with a
/// ready-to-serve value.
pub fn build_app(state: AppState) -> Router {
    // Extract values we need before moving state into the router.
    let cors_layer = mw::cors_layer(&state.config.cors);
    let timeout_secs = state.config.http.request_timeout_secs;

    // Compose the OpenAPI router from every domain module. `split_for_parts`
    // separates the Axum routes (for serving) from the merged OpenAPI schema
    // (for the docs UI).
    let (api_router, openapi) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .merge(health::routes::router())
        .split_for_parts();

    let docs_router = crate::openapi::docs_router(openapi);

    // Security response headers — applied as a single stack for readability.
    let security_headers = ServiceBuilder::new()
        .layer(mw::hsts_layer())
        .layer(mw::csp_layer())
        .layer(mw::nosniff_layer())
        .layer(mw::frame_options_layer())
        .layer(mw::referrer_layer())
        .layer(mw::permissions_policy_layer())
        .layer(mw::coop_layer())
        .layer(mw::corp_layer());

    let (sensitive_req, sensitive_resp) = mw::sensitive_headers();

    Router::new()
        .merge(api_router)
        .merge(docs_router)
        // Innermost first → outermost last.
        .layer(security_headers)
        .layer(cors_layer)
        .layer(mw::compression_layer())
        .layer(mw::timeout_layer(timeout_secs))
        .layer(mw::propagate_request_id_layer())
        .layer(mw::traceparent_layer())
        .layer(sensitive_resp)
        .layer(mw::trace_layer())
        .layer(sensitive_req)
        .layer(mw::set_request_id_layer())
        .layer(mw::panic_layer())
        .with_state(state)
}
