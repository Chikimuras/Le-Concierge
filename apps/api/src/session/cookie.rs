//! Helpers for building and clearing the session cookie.
//!
//! Attributes follow `CLAUDE.md` §3.1 and ADR 0006:
//!
//! - `HttpOnly` — never accessible from JavaScript.
//! - `Secure` — only sent over HTTPS (toggled off in local HTTP dev).
//! - `SameSite=Lax` — sent on top-level navigations but not cross-site
//!   embedded requests. Strict enough for our endpoints while still
//!   letting the auth cookie flow on normal login redirects.
//! - `Path=/` — active for the entire app.
//! - `Max-Age` — matches the idle TTL so the browser drops the cookie at
//!   the same time Redis would drop the session entry.

use std::time::Duration;

use axum_extra::extract::cookie::{Cookie, SameSite};

use crate::session::dto::SessionId;

/// Cookie name. Short, project-prefixed, non-PII.
pub const SESSION_COOKIE_NAME: &str = "lc_sid";

/// Config-derived cookie attributes. Populated at `AppState::new` time.
#[derive(Debug, Clone)]
pub struct CookieConfig {
    /// `Secure` attribute. `false` only for local HTTP dev.
    pub secure: bool,
    /// `Domain` attribute. `None` pins the cookie to the origin host.
    pub domain: Option<String>,
}

/// Build the `Set-Cookie` header carrying a freshly issued session.
#[must_use]
pub fn session_cookie(
    sid: &SessionId,
    max_age: Duration,
    config: &CookieConfig,
) -> Cookie<'static> {
    build(sid.as_str().to_owned(), max_age, config)
}

/// Build an *expiring* `Set-Cookie` header that tells the browser to drop
/// any existing `lc_sid`. Sent on logout.
#[must_use]
pub fn clear_session_cookie(config: &CookieConfig) -> Cookie<'static> {
    build(String::new(), Duration::from_secs(0), config)
}

fn build(value: String, max_age: Duration, config: &CookieConfig) -> Cookie<'static> {
    let max_age_sec = i64::try_from(max_age.as_secs()).unwrap_or(i64::MAX);
    let mut builder = Cookie::build((SESSION_COOKIE_NAME, value))
        .http_only(true)
        .secure(config.secure)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(time::Duration::seconds(max_age_sec));
    if let Some(domain) = &config.domain {
        builder = builder.domain(domain.clone());
    }
    builder.build()
}
