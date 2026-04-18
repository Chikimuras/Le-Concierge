//! Double-submit CSRF guard for state-changing methods.
//!
//! Safe methods (GET, HEAD, OPTIONS, TRACE) pass through untouched. Any
//! other method must carry an `X-CSRF-Token` header whose value matches
//! the `csrf_token` stored inside the caller's session (OWASP Cheat
//! Sheet — Cross-Site Request Forgery Prevention).
//!
//! Requests without a valid session cookie are left alone here — the
//! auth extractor on the handler will produce the 401 if authentication
//! is required, or the route stays anonymous. The CSRF check only bites
//! when a session cookie IS present, because that is when a CSRF attack
//! would actually authenticate.

use axum::{
    extract::{Request, State},
    http::{HeaderName, Method, StatusCode},
    middleware::Next,
    response::Response,
};
use axum_extra::extract::CookieJar;

use crate::{
    AppState,
    session::{cookie::SESSION_COOKIE_NAME, dto::SessionId},
};

pub const CSRF_HEADER: HeaderName = HeaderName::from_static("x-csrf-token");

/// Axum middleware that enforces the CSRF policy described at module
/// level. Attach it with `axum::middleware::from_fn_with_state`.
pub async fn guard(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if is_safe(request.method()) {
        return Ok(next.run(request).await);
    }

    let jar = CookieJar::from_headers(request.headers());
    let Some(cookie_value) = jar.get(SESSION_COOKIE_NAME).map(|c| c.value().to_owned()) else {
        // No session — no authenticated CSRF target. Let the handler
        // decide (likely a 401 via the auth extractor).
        return Ok(next.run(request).await);
    };

    let sid = match SessionId::parse(&cookie_value) {
        Ok(sid) => sid,
        // Malformed cookie: surface as 403 rather than 401 so the caller
        // knows it's the CSRF gate, not an auth failure.
        Err(_) => return Err(StatusCode::FORBIDDEN),
    };

    let Ok(session) = state.session.lookup(&sid).await else {
        return Err(StatusCode::FORBIDDEN);
    };

    let header_value = request
        .headers()
        .get(&CSRF_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !session.csrf_matches(header_value) {
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(next.run(request).await)
}

fn is_safe(method: &Method) -> bool {
    matches!(
        *method,
        Method::GET | Method::HEAD | Method::OPTIONS | Method::TRACE
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_methods_bypass_csrf() {
        assert!(is_safe(&Method::GET));
        assert!(is_safe(&Method::HEAD));
        assert!(is_safe(&Method::OPTIONS));
        assert!(is_safe(&Method::TRACE));
    }

    #[test]
    fn unsafe_methods_are_guarded() {
        assert!(!is_safe(&Method::POST));
        assert!(!is_safe(&Method::PUT));
        assert!(!is_safe(&Method::PATCH));
        assert!(!is_safe(&Method::DELETE));
    }
}
