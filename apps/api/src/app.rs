//! Compose the Axum router: domain routes, OpenAPI docs, and middleware.
//!
//! Middleware stack (inner → outer, i.e. order of execution on the response
//! path):
//!
//! 1. Route-local layers (Scalar CSP, per-router rate limit, future auth).
//! 2. CSRF guard on unsafe methods (see [`crate::session::csrf`]).
//! 3. Security response headers (HSTS, CSP, nosniff, frame-deny, referrer,
//!    permissions, COOP, CORP).
//! 4. CORS.
//! 5. Compression.
//! 6. Request timeout.
//! 7. `x-request-id` / `traceparent` propagation.
//! 8. Sensitive response / request header redaction for tracing.
//! 9. HTTP request tracing span.
//! 10. `x-request-id` generation.
//! 11. Panic catch (outermost).
//!
//! Request flow is the reverse: panic catch first, handler last.

use std::sync::Arc;

use axum::{Router, middleware::from_fn_with_state};
use tower::ServiceBuilder;
use tower_governor::{
    GovernorLayer, governor::GovernorConfigBuilder, key_extractor::SmartIpKeyExtractor,
};
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;

use crate::{auth, health, middleware as mw, openapi::ApiDoc, session::csrf, state::AppState};

/// Build the fully composed application router.
pub fn build_app(state: AppState) -> Router {
    let cors_layer = mw::cors_layer(&state.config.cors);
    let timeout_secs = state.config.http.request_timeout_secs;

    // Rate limit for /auth/signup + /auth/login: 5-request burst, then one
    // replenish every 3 minutes (≈ 5 / 15 min / IP, CLAUDE.md §3.1).
    // SmartIpKeyExtractor honours X-Forwarded-For when a reverse proxy
    // sets it, otherwise falls back to the socket peer.
    //
    // `expect` is legitimate here: the builder arguments are compile-time
    // constants, `.finish()` can only return `None` when they are out of
    // range — which they are not. Allowing the lint keeps the bootstrap
    // readable (CLAUDE.md §7.1 permits `expect` in bootstrap code).
    #[allow(clippy::expect_used)]
    let auth_rl_config = Arc::new(
        GovernorConfigBuilder::default()
            .per_millisecond(180_000)
            .burst_size(5)
            .key_extractor(SmartIpKeyExtractor)
            .finish()
            .expect("static rate-limit config always builds"),
    );

    // Compose routes with OpenAPI metadata, then split for serving.
    let (api_router, openapi) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .merge(health::routes::router())
        .merge(auth::routes::anonymous_router().layer(GovernorLayer {
            config: auth_rl_config,
        }))
        .merge(auth::routes::authenticated_router())
        .split_for_parts();

    let docs_router = crate::openapi::docs_router(openapi);

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
        // CSRF runs *inside* the security headers so a 403 still carries
        // our baseline HSTS/CSP/etc. Plus it needs access to `state` to
        // look sessions up in Redis.
        .layer(from_fn_with_state(state.clone(), csrf::guard))
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
