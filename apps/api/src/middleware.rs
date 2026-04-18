//! Global HTTP middleware layer builders.
//!
//! Each function returns a concrete `tower` layer. Composition lives in
//! [`crate::app::build_app`] so the middleware stack is visible in one place.
//!
//! Security controls applied here follow `CLAUDE.md` §3.2 and ADR 0002:
//!
//! - HSTS, CSP, nosniff, frame-deny, referrer, permissions headers on every
//!   response (OWASP ASVS 14.4).
//! - `x-request-id` inbound/outbound for request correlation.
//! - W3C `traceparent` propagation (<https://www.w3.org/TR/trace-context/>).
//! - CORS from explicit allow-list, no wildcard with credentials
//!   (ASVS 14.5.3).
//! - Response compression, request timeout, panic catching.
//! - Sensitive headers (`authorization`, `cookie`, …) redacted from tracing.

use std::{sync::Arc, time::Duration};

use axum::http::{HeaderName, HeaderValue, Method, StatusCode, header};
use tower_http::{
    catch_panic::CatchPanicLayer,
    compression::CompressionLayer,
    cors::{AllowOrigin, CorsLayer},
    propagate_header::PropagateHeaderLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    sensitive_headers::{SetSensitiveRequestHeadersLayer, SetSensitiveResponseHeadersLayer},
    set_header::SetResponseHeaderLayer,
    timeout::TimeoutLayer,
    trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer},
};
use tracing::Level;

use crate::config::CorsConfig;

pub const REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");
pub const TRACEPARENT_HEADER: HeaderName = HeaderName::from_static("traceparent");

// --- Security response headers ----------------------------------------------

/// `Strict-Transport-Security`. Two years + preload + subdomains per
/// CLAUDE.md §3.2.
#[must_use]
pub fn hsts_layer() -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::overriding(
        header::STRICT_TRANSPORT_SECURITY,
        HeaderValue::from_static("max-age=63072000; includeSubDomains; preload"),
    )
}

/// Baseline CSP for JSON API responses: everything denied.
///
/// Uses `if_not_present` so routes that legitimately need a different CSP
/// (notably `/docs`, which serves Scalar's HTML UI) can set their own via
/// an inner layer before this one runs. If the inner layer is ever removed
/// by accident, the default here re-applies — safe fail mode.
#[must_use]
pub fn csp_layer() -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::if_not_present(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
        ),
    )
}

#[must_use]
pub fn nosniff_layer() -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::overriding(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    )
}

#[must_use]
pub fn frame_options_layer() -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::overriding(
        HeaderName::from_static("x-frame-options"),
        HeaderValue::from_static("DENY"),
    )
}

#[must_use]
pub fn referrer_layer() -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::overriding(
        header::REFERRER_POLICY,
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    )
}

#[must_use]
pub fn permissions_policy_layer() -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::overriding(
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static(
            "accelerometer=(), autoplay=(), camera=(), geolocation=(), gyroscope=(), \
             microphone=(), payment=(), usb=()",
        ),
    )
}

#[must_use]
pub fn coop_layer() -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::overriding(
        HeaderName::from_static("cross-origin-opener-policy"),
        HeaderValue::from_static("same-origin"),
    )
}

#[must_use]
pub fn corp_layer() -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::overriding(
        HeaderName::from_static("cross-origin-resource-policy"),
        HeaderValue::from_static("same-site"),
    )
}

// --- Tracing / request correlation ------------------------------------------

/// Redact credentials from `tracing` spans so they never hit the log sink.
#[must_use]
pub fn sensitive_headers() -> (
    SetSensitiveRequestHeadersLayer,
    SetSensitiveResponseHeadersLayer,
) {
    let sensitive: Arc<[HeaderName]> = Arc::from(
        [
            header::AUTHORIZATION,
            header::COOKIE,
            header::SET_COOKIE,
            HeaderName::from_static("x-api-key"),
        ]
        .as_slice(),
    );
    (
        SetSensitiveRequestHeadersLayer::from_shared(sensitive.clone()),
        SetSensitiveResponseHeadersLayer::from_shared(sensitive),
    )
}

/// One INFO span per HTTP request. Fields flow into the JSON log formatter.
#[must_use]
pub fn trace_layer()
-> TraceLayer<tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>>
{
    TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
        .on_request(DefaultOnRequest::new().level(Level::INFO))
        .on_response(DefaultOnResponse::new().level(Level::INFO))
}

/// Set and propagate `x-request-id` on every request/response.
#[must_use]
pub fn set_request_id_layer() -> SetRequestIdLayer<MakeRequestUuid> {
    SetRequestIdLayer::new(REQUEST_ID_HEADER, MakeRequestUuid)
}

#[must_use]
pub fn propagate_request_id_layer() -> PropagateRequestIdLayer {
    PropagateRequestIdLayer::new(REQUEST_ID_HEADER)
}

/// Forward the W3C `traceparent` header to the response for front-to-API
/// correlation.
#[must_use]
pub fn traceparent_layer() -> PropagateHeaderLayer {
    PropagateHeaderLayer::new(TRACEPARENT_HEADER)
}

// --- Runtime limits ---------------------------------------------------------

#[must_use]
pub fn timeout_layer(secs: u64) -> TimeoutLayer {
    // `TimeoutLayer::new` was deprecated in recent tower-http in favour of
    // the explicit-status-code variant. 408 REQUEST_TIMEOUT matches what
    // `new` used to emit, so behaviour is unchanged.
    TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, Duration::from_secs(secs))
}

#[must_use]
pub fn compression_layer() -> CompressionLayer {
    CompressionLayer::new().gzip(true).br(true).no_deflate()
}

#[must_use]
pub fn panic_layer() -> CatchPanicLayer<tower_http::catch_panic::DefaultResponseForPanic> {
    CatchPanicLayer::new()
}

// --- CORS -------------------------------------------------------------------

/// Build a CORS layer from config. Empty allow-list → CORS effectively off.
pub fn cors_layer(config: &CorsConfig) -> CorsLayer {
    let origins: Vec<HeaderValue> = config
        .allowed_origins
        .iter()
        .filter_map(|o| HeaderValue::from_str(o).ok())
        .collect();

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            header::ACCEPT,
            TRACEPARENT_HEADER,
        ])
        .allow_credentials(true)
        .max_age(Duration::from_secs(600))
}
