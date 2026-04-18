//! HTTP handlers for `/auth/*`.
//!
//! Cookie life cycle:
//!
//! - `POST /auth/signup` — creates org + owner, mints a session, sets `lc_sid`.
//! - `POST /auth/login`  — verifies credentials, mints a session, sets `lc_sid`.
//! - `POST /auth/logout` — destroys the session, expires `lc_sid`.
//! - `GET  /auth/me`     — returns the current session metadata + user context.
//!
//! CSRF: `/auth/signup` and `/auth/login` run **before** a session exists,
//! so the CSRF middleware is a no-op on them. `/auth/logout` is protected
//! because it requires an authenticated caller — the middleware checks
//! the `X-CSRF-Token` header against the session's secret.

use std::net::{IpAddr, SocketAddr};

use axum::{
    Json,
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode, header},
};
use axum_extra::extract::CookieJar;
use chrono::Utc;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppError, AppState,
    auth::{
        Role,
        dto::{AuthenticatedResponse, LoginRequest, MeResponse, MembershipSummary, SignupRequest},
        service::{LoginInput, SignupInput, UserContext},
    },
    session::{AuthenticatedUser, clear_session_cookie, session_cookie},
};

/// Create a new organization and its first user, then log them in.
///
/// Sets the `lc_sid` cookie on success. The body echoes the fresh
/// `csrf_token` — the frontend stores it and sends it back in the
/// `X-CSRF-Token` header on every subsequent state-changing call.
#[utoipa::path(
    post,
    path = "/auth/signup",
    tag = "auth",
    request_body = SignupRequest,
    responses(
        (status = 200, description = "Signed up and logged in", body = AuthenticatedResponse),
        (status = 409, description = "Email or slug already taken"),
        (status = 422, description = "Validation error"),
        (status = 429, description = "Rate limited"),
    )
)]
pub async fn signup(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    jar: CookieJar,
    Json(input): Json<SignupRequest>,
) -> Result<(CookieJar, Json<AuthenticatedResponse>), AppError> {
    let outcome = state
        .auth
        .signup_organization(SignupInput {
            email: input.email.clone(),
            password: input.password,
            organization_slug: input.organization_slug,
            organization_name: input.organization_name,
        })
        .await?;

    let ua = user_agent(&headers);
    let ip = real_client_ip(&headers, addr.ip());

    // Auto-login: mint a session for the owner so the frontend skips the
    // extra login round-trip right after signup.
    let issue = state
        .auth
        .sessions()
        .create(outcome.owner_user_id, ip, ua)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let context = state.auth.load_user_context(outcome.owner_user_id).await?;

    let cookie = session_cookie(&issue.0, state.session_idle_ttl(), &state.cookie_config());
    let jar = jar.add(cookie);

    let response = AuthenticatedResponse {
        session: (&issue.1).into(),
        user_id: outcome.owner_user_id,
        mfa_required: mfa_required_for(&context),
        // Brand-new user cannot have 2FA enrolled yet.
        mfa_enrolled: false,
        memberships: to_membership_summaries(context.memberships),
        is_platform_admin: context.is_platform_admin,
    };
    Ok((jar, Json(response)))
}

/// Verify credentials and mint a session.
#[utoipa::path(
    post,
    path = "/auth/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Logged in", body = AuthenticatedResponse),
        (status = 401, description = "Invalid credentials"),
        (status = 429, description = "Rate limited"),
    )
)]
pub async fn login(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    jar: CookieJar,
    Json(input): Json<LoginRequest>,
) -> Result<(CookieJar, Json<AuthenticatedResponse>), AppError> {
    let ua = user_agent(&headers);
    let ip = real_client_ip(&headers, addr.ip());

    let (user_id, issue) = state
        .auth
        .login_with_password(
            LoginInput {
                email: input.email,
                password: input.password,
            },
            ip,
            ua,
        )
        .await?;

    let context = state.auth.load_user_context(user_id).await?;
    let mfa_enrolled = state.totp.is_enrolled(user_id).await?;
    let mfa_required = mfa_required_for(&context);

    let cookie = session_cookie(
        &issue.session_id,
        state.session_idle_ttl(),
        &state.cookie_config(),
    );
    let jar = jar.add(cookie);

    let response = AuthenticatedResponse {
        session: (&issue.session).into(),
        user_id,
        memberships: to_membership_summaries(context.memberships),
        is_platform_admin: context.is_platform_admin,
        mfa_enrolled,
        mfa_required,
    };
    Ok((jar, Json(response)))
}

/// Destroy the current session and clear the cookie.
#[utoipa::path(
    post,
    path = "/auth/logout",
    tag = "auth",
    responses(
        (status = 204, description = "Logged out"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "CSRF token missing or invalid"),
    )
)]
pub async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
    auth: AuthenticatedUser,
) -> Result<(CookieJar, StatusCode), AppError> {
    state.auth.logout(&auth.session_id, auth.user_id).await?;
    let jar = jar.add(clear_session_cookie(&state.cookie_config()));
    Ok((jar, StatusCode::NO_CONTENT))
}

/// Inspect the current session.
#[utoipa::path(
    get,
    path = "/auth/me",
    tag = "auth",
    responses(
        (status = 200, description = "Current session", body = MeResponse),
        (status = 401, description = "Not authenticated"),
    )
)]
pub async fn me(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<MeResponse>, AppError> {
    let context = state.auth.load_user_context(auth.user_id).await?;
    let mfa_enrolled = state.totp.is_enrolled(auth.user_id).await?;
    let mfa_required = mfa_required_for(&context);
    Ok(Json(MeResponse {
        session: (&auth.session).into(),
        user_id: auth.user_id,
        memberships: to_membership_summaries(context.memberships),
        is_platform_admin: context.is_platform_admin,
        mfa_enrolled,
        mfa_required,
        resolved_at: Utc::now(),
    }))
}

/// Router for *anonymous* auth endpoints (no session cookie yet).
/// Caller is expected to wrap this with a rate-limit layer — see
/// [`crate::app::build_app`].
#[must_use]
pub fn anonymous_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(signup))
        .routes(routes!(login))
}

/// Router for auth endpoints that require an authenticated caller. The
/// `AuthenticatedUser` extractor on each handler provides the 401 gate.
#[must_use]
pub fn authenticated_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(logout))
        .routes(routes!(me))
}

// ---- Helpers ---------------------------------------------------------------

pub(crate) fn user_agent(headers: &HeaderMap) -> &str {
    headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
}

/// Prefer the first `X-Forwarded-For` entry (set by Caddy / Traefik in
/// front of the API) over the direct socket. When the API is exposed
/// directly, XFF is caller-controlled and should not be trusted — add a
/// `trust_proxy: bool` config flag when that deployment topology appears.
pub(crate) fn real_client_ip(headers: &HeaderMap, fallback: IpAddr) -> IpAddr {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(fallback)
}

/// Compute `mfa_required` from a hydrated [`UserContext`]. CLAUDE.md §3.1
/// makes 2FA mandatory for platform admins and for per-org `manager`s;
/// `owner` can opt in but is not forced; `cleaner` / `guest` are never
/// required.
pub(crate) fn mfa_required_for(context: &UserContext) -> bool {
    context.is_platform_admin || context.memberships.iter().any(|m| m.role == Role::Manager)
}

fn to_membership_summaries(rows: Vec<crate::auth::repo::MembershipRow>) -> Vec<MembershipSummary> {
    rows.into_iter()
        .map(|r| MembershipSummary {
            org_id: r.org_id,
            org_slug: r.org_slug,
            org_name: r.org_name,
            role: r.role,
        })
        .collect()
}
