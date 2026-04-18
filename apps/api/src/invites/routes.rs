//! HTTP handlers for team invites. Split into two routers:
//!
//! - `authenticated_router` hosts the manager-side endpoints under
//!   `/orgs/:slug/invites(/:id)` and relies on the [`Membership`]
//!   extractor for tenant resolution + role gating.
//! - `anonymous_router` hosts the invitee-side endpoints under
//!   `/auth/invites/{preview,accept,signup}`. Rate limit is applied
//!   in `app.rs` (same 5/3-min budget as `/auth/login`).

use std::net::SocketAddr;

use axum::{
    Json,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, StatusCode},
};
use axum_extra::extract::CookieJar;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use crate::{
    AppError, AppState,
    auth::{
        Role, Slug,
        domain::Email,
        dto::{AuthenticatedResponse, MembershipSummary},
        routes::{mfa_required_for, real_client_ip, user_agent},
    },
    invites::{
        domain::{CreateInviteInput, InviteId},
        dto::{
            AcceptRequest, CreateInviteRequest, InviteListResponse, InviteResponse, PreviewRequest,
            PreviewResponse, SignupAndAcceptRequest,
        },
        service::AcceptOutcome,
    },
    session::{AuthenticatedUser, session_cookie},
    tenancy::Membership,
};

// ---- Manager-side --------------------------------------------------------

#[utoipa::path(
    get,
    path = "/orgs/{slug}/invites",
    tag = "invites",
    params(("slug" = String, Path, description = "Organisation slug")),
    responses(
        (status = 200, description = "Pending invites", body = InviteListResponse),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Org not found or insufficient role"),
    )
)]
pub async fn list_invites(
    State(state): State<AppState>,
    access: Membership,
) -> Result<Json<InviteListResponse>, AppError> {
    access.ensure_role(Role::Manager)?;
    let rows = state.invites.list_pending(access.org_id).await?;
    Ok(Json(InviteListResponse {
        invites: rows.into_iter().map(InviteResponse::from).collect(),
    }))
}

#[utoipa::path(
    post,
    path = "/orgs/{slug}/invites",
    tag = "invites",
    params(("slug" = String, Path, description = "Organisation slug")),
    request_body = CreateInviteRequest,
    responses(
        (status = 201, description = "Invite created", body = InviteResponse),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "CSRF token missing / invalid"),
        (status = 404, description = "Org not found or insufficient role"),
        (status = 409, description = "A pending invite already exists for that email"),
        (status = 422, description = "Invalid input"),
    )
)]
pub async fn create_invite(
    State(state): State<AppState>,
    Path(slug_raw): Path<String>,
    access: Membership,
    Json(body): Json<CreateInviteRequest>,
) -> Result<(StatusCode, Json<InviteResponse>), AppError> {
    access.ensure_role(Role::Manager)?;
    let email = Email::parse(&body.email)?;
    let slug = Slug::parse(&slug_raw)?;
    let org_name = org_name_for(&state, access.org_id).await?;
    let invite = state
        .invites
        .create(
            access.user_id,
            access.org_id,
            &slug,
            &org_name,
            CreateInviteInput {
                email,
                role: body.role,
            },
        )
        .await?;
    Ok((StatusCode::CREATED, Json(InviteResponse::from(invite))))
}

#[utoipa::path(
    delete,
    path = "/orgs/{slug}/invites/{id}",
    tag = "invites",
    params(
        ("slug" = String, Path, description = "Organisation slug"),
        ("id" = Uuid, Path, description = "Invite id"),
    ),
    responses(
        (status = 204, description = "Invite cancelled"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "CSRF token missing / invalid"),
        (status = 404, description = "Invite not found or insufficient role"),
    )
)]
pub async fn cancel_invite(
    State(state): State<AppState>,
    access: Membership,
    Path((_slug, id)): Path<(String, Uuid)>,
) -> Result<StatusCode, AppError> {
    access.ensure_role(Role::Manager)?;
    state
        .invites
        .cancel(access.user_id, access.org_id, InviteId::from(id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- Invitee-side --------------------------------------------------------

#[utoipa::path(
    post,
    path = "/auth/invites/preview",
    tag = "invites",
    request_body = PreviewRequest,
    responses(
        (status = 200, description = "Invite metadata for display", body = PreviewResponse),
        (status = 404, description = "Invite not found / cancelled"),
        (status = 410, description = "Invite expired"),
        (status = 429, description = "Rate limited"),
    )
)]
pub async fn preview_invite(
    State(state): State<AppState>,
    Json(body): Json<PreviewRequest>,
) -> Result<Json<PreviewResponse>, AppError> {
    let preview = state.invites.preview(&body.token).await?;
    Ok(Json(PreviewResponse::from(preview)))
}

#[utoipa::path(
    post,
    path = "/auth/invites/accept",
    tag = "invites",
    request_body = AcceptRequest,
    responses(
        (status = 200, description = "Membership added", body = AuthenticatedResponse),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "CSRF token missing / invalid"),
        (status = 404, description = "Invite not found or email mismatch"),
        (status = 410, description = "Invite expired"),
        (status = 429, description = "Rate limited"),
    )
)]
pub async fn accept_invite(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(body): Json<AcceptRequest>,
) -> Result<Json<AuthenticatedResponse>, AppError> {
    let email = state.auth.load_email(auth.user_id).await?;
    let outcome = state
        .invites
        .accept_as(auth.user_id, &email, &body.token)
        .await?;
    let AcceptOutcome::MembershipAdded { user_id } = outcome else {
        // accept_as never issues a new session — the match is
        // exhaustive in practice but the compiler insists.
        return Err(AppError::Internal(anyhow::anyhow!(
            "accept_as returned an unexpected outcome"
        )));
    };
    let response = build_authed_response(&state, user_id, &auth.session).await?;
    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = "/auth/invites/signup",
    tag = "invites",
    request_body = SignupAndAcceptRequest,
    responses(
        (status = 200, description = "User created + membership added, session issued", body = AuthenticatedResponse),
        (status = 404, description = "Invite not found"),
        (status = 409, description = "Email already registered"),
        (status = 410, description = "Invite expired"),
        (status = 422, description = "Weak password"),
        (status = 429, description = "Rate limited"),
    )
)]
pub async fn signup_and_accept(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    jar: CookieJar,
    Json(body): Json<SignupAndAcceptRequest>,
) -> Result<(CookieJar, Json<AuthenticatedResponse>), AppError> {
    let ip = real_client_ip(&headers, addr.ip());
    let ua = user_agent(&headers);

    let outcome = state
        .invites
        .signup_and_accept(&body.token, &body.password, ip, ua)
        .await?;
    let AcceptOutcome::NewSession {
        user_id,
        session_id,
        session,
    } = outcome
    else {
        return Err(AppError::Internal(anyhow::anyhow!(
            "signup_and_accept returned an unexpected outcome"
        )));
    };

    let jar = jar.add(session_cookie(
        &session_id,
        state.session_idle_ttl(),
        &state.cookie_config(),
    ));
    let response = build_authed_response(&state, user_id, &session).await?;
    Ok((jar, Json(response)))
}

// ---- helpers -------------------------------------------------------------

async fn org_name_for(state: &AppState, org_id: crate::auth::OrgId) -> Result<String, AppError> {
    sqlx::query_scalar!(
        r#"SELECT name FROM organizations WHERE id = $1"#,
        org_id.into_inner(),
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .ok_or(AppError::NotFound)
}

async fn build_authed_response(
    state: &AppState,
    user_id: crate::auth::UserId,
    session: &crate::session::SessionData,
) -> Result<AuthenticatedResponse, AppError> {
    let context = state.auth.load_user_context(user_id).await?;
    let mfa_enrolled = state.totp.is_enrolled(user_id).await?;
    let mfa_required = mfa_required_for(&context);
    Ok(AuthenticatedResponse {
        session: session.into(),
        user_id,
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
        mfa_enrolled,
        mfa_required,
    })
}

#[must_use]
pub fn authenticated_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_invites, create_invite))
        .routes(routes!(cancel_invite))
        .routes(routes!(accept_invite))
}

#[must_use]
pub fn anonymous_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(preview_invite))
        .routes(routes!(signup_and_accept))
}
