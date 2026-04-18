//! HTTP handlers for `/auth/2fa/*`.
//!
//! - `POST /auth/2fa/enroll/start`  — pending enrollment, returns
//!   `otpauth_url` + `secret_base32`.
//! - `POST /auth/2fa/enroll/verify` — finalise, returns recovery codes
//!   exactly once.
//! - `POST /auth/2fa/verify`        — step-up: rotates the session and
//!   sets `mfa_verified = true`.
//! - `POST /auth/2fa/disable`       — requires password + current TOTP;
//!   refuses to run on a session that has not cleared step-up.
//!
//! Session rotation on verify lives here (not in the service) because
//! only the HTTP layer holds the pre-MFA `SessionId` and the cookie
//! jar it needs to mutate.

use std::net::SocketAddr;

use axum::{
    Json,
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode},
};
use axum_extra::extract::CookieJar;

use crate::{
    AppError, AppState,
    auth::{
        dto::{AuthenticatedResponse, MembershipSummary},
        routes::{mfa_required_for, real_client_ip, user_agent},
        totp::{
            TotpVerifyOutcome,
            dto::{
                DisableRequest, EnrollStartResponse, EnrollVerifyRequest, EnrollVerifyResponse,
                VerifyRequest, VerifyResponse,
            },
        },
    },
    session::{AuthenticatedUser, clear_session_cookie, session_cookie},
};

#[utoipa::path(
    post,
    path = "/auth/2fa/enroll/start",
    tag = "auth",
    responses(
        (status = 200, description = "Pending enrollment created", body = EnrollStartResponse),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "CSRF token missing or invalid"),
        (status = 409, description = "Already enrolled"),
        (status = 429, description = "Rate limited"),
    )
)]
pub async fn enroll_start(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    auth: AuthenticatedUser,
) -> Result<Json<EnrollStartResponse>, AppError> {
    let email = state.auth.load_email(auth.user_id).await?;
    let ip = real_client_ip(&headers, addr.ip());
    let ua = user_agent(&headers);

    let outcome = state
        .totp
        .start_enrollment(auth.user_id, &email, ip, ua)
        .await?;

    Ok(Json(EnrollStartResponse {
        otpauth_url: outcome.otpauth_url,
        secret_base32: outcome.secret_base32,
    }))
}

#[utoipa::path(
    post,
    path = "/auth/2fa/enroll/verify",
    tag = "auth",
    request_body = EnrollVerifyRequest,
    responses(
        (status = 200, description = "2FA enrolled, recovery codes returned once", body = EnrollVerifyResponse),
        (status = 400, description = "No pending enrollment"),
        (status = 401, description = "Wrong TOTP code / not authenticated"),
        (status = 403, description = "CSRF token missing or invalid"),
        (status = 429, description = "Rate limited"),
    )
)]
pub async fn enroll_verify(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    auth: AuthenticatedUser,
    Json(input): Json<EnrollVerifyRequest>,
) -> Result<Json<EnrollVerifyResponse>, AppError> {
    let email = state.auth.load_email(auth.user_id).await?;
    let ip = real_client_ip(&headers, addr.ip());
    let ua = user_agent(&headers);

    let codes = state
        .totp
        .confirm_enrollment(auth.user_id, &email, input.code.trim(), ip, ua)
        .await?;

    Ok(Json(EnrollVerifyResponse {
        recovery_codes: codes.into_iter().map(|c| c.0).collect(),
    }))
}

#[utoipa::path(
    post,
    path = "/auth/2fa/verify",
    tag = "auth",
    request_body = VerifyRequest,
    responses(
        (status = 200, description = "MFA cleared, session rotated", body = VerifyResponse),
        (status = 400, description = "2FA not enrolled"),
        (status = 401, description = "Wrong code / not authenticated"),
        (status = 403, description = "CSRF token missing or invalid"),
        (status = 429, description = "Rate limited"),
    )
)]
pub async fn verify(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    jar: CookieJar,
    auth: AuthenticatedUser,
    Json(input): Json<VerifyRequest>,
) -> Result<(CookieJar, Json<VerifyResponse>), AppError> {
    let email = state.auth.load_email(auth.user_id).await?;
    let ip = real_client_ip(&headers, addr.ip());
    let ua = user_agent(&headers);

    let outcome = state
        .totp
        .verify_code(auth.user_id, &email, input.code.trim(), ip, ua)
        .await?;

    let (new_sid, new_session) = state
        .session
        .verify_mfa(&auth.session_id, auth.user_id, ip, ua)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let context = state.auth.load_user_context(auth.user_id).await?;
    let mfa_required = mfa_required_for(&context);

    let jar = jar.add(session_cookie(
        &new_sid,
        state.session_idle_ttl(),
        &state.cookie_config(),
    ));

    let body = VerifyResponse {
        session: AuthenticatedResponse {
            session: (&new_session).into(),
            user_id: auth.user_id,
            memberships: context
                .memberships
                .into_iter()
                .map(|m| MembershipSummary {
                    org_id: m.org_id,
                    org_slug: m.org_slug,
                    org_name: m.org_name,
                    role: m.role,
                })
                .collect(),
            is_platform_admin: context.is_platform_admin,
            // The user is here precisely because they have an active
            // enrollment — no DB round-trip to recheck.
            mfa_enrolled: true,
            mfa_required,
        },
        used_recovery_code: matches!(outcome, TotpVerifyOutcome::RecoveryCode),
    };
    Ok((jar, Json(body)))
}

#[utoipa::path(
    post,
    path = "/auth/2fa/disable",
    tag = "auth",
    request_body = DisableRequest,
    responses(
        (status = 204, description = "2FA disabled, session left intact"),
        (status = 400, description = "2FA not enrolled"),
        (status = 401, description = "Wrong password or code / not authenticated"),
        (status = 403, description = "CSRF token missing or invalid, or MFA not yet cleared"),
        (status = 429, description = "Rate limited"),
    )
)]
pub async fn disable(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    jar: CookieJar,
    auth: AuthenticatedUser,
    Json(input): Json<DisableRequest>,
) -> Result<(CookieJar, StatusCode), AppError> {
    // Defence in depth: disable must run on an MFA-verified session. The
    // frontend should have taken the user through step-up first; deny
    // here if it somehow didn't.
    if !auth.session.mfa_verified {
        return Err(AppError::Forbidden);
    }

    let email = state.auth.load_email(auth.user_id).await?;

    // Re-verify the password — opaquely, no counter change (verifying a
    // password during disable is not a login attempt).
    let password_ok = state
        .auth
        .verify_password_for_user(auth.user_id, &input.password)
        .await?;
    if !password_ok {
        return Err(AppError::Unauthorized);
    }

    let ip = real_client_ip(&headers, addr.ip());
    let ua = user_agent(&headers);
    state
        .totp
        .disable(auth.user_id, &email, input.code.trim(), ip, ua)
        .await?;

    // Disabling 2FA should downgrade the session's `mfa_verified` flag,
    // so a stolen cookie cannot masquerade as a fully-stepped-up
    // session. We destroy the current session outright — the user logs
    // back in without the 2FA prompt on the next request.
    state
        .session
        .destroy(&auth.session_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let jar = jar.add(clear_session_cookie(&state.cookie_config()));

    Ok((jar, StatusCode::NO_CONTENT))
}

#[must_use]
pub fn router() -> utoipa_axum::router::OpenApiRouter<AppState> {
    use utoipa_axum::{router::OpenApiRouter, routes};
    OpenApiRouter::new()
        .routes(routes!(enroll_start))
        .routes(routes!(enroll_verify))
        .routes(routes!(verify))
        .routes(routes!(disable))
}
