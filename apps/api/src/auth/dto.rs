//! Request / response DTOs for the `/auth/*` HTTP endpoints.
//!
//! Every input deserializer uses `#[serde(deny_unknown_fields)]`
//! (CLAUDE.md §3.4 / OWASP ASVS 5.1.5) — silent drift in client payloads
//! becomes a 4xx rather than a mystery bug.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    auth::{OrgId, Role, UserId},
    session::SessionMeta,
};

// ---- Shared ----------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MembershipSummary {
    pub org_id: OrgId,
    #[schema(example = "acme")]
    pub org_slug: String,
    #[schema(example = "Acme Conciergerie")]
    pub org_name: String,
    pub role: Role,
}

/// Body returned by successful authentication endpoints. The session
/// cookie is set as a side-effect on the response headers.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AuthenticatedResponse {
    pub session: SessionMeta,
    pub user_id: UserId,
    pub memberships: Vec<MembershipSummary>,
    pub is_platform_admin: bool,
}

// ---- Signup ----------------------------------------------------------------

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SignupRequest {
    #[schema(example = "owner@example.com")]
    pub email: String,
    /// Minimum 12 characters.
    #[schema(format = "password", min_length = 12)]
    pub password: String,
    /// Organization slug (lowercase kebab-case, 2–64 chars).
    #[schema(example = "acme", min_length = 2, max_length = 64)]
    pub organization_slug: String,
    #[schema(example = "Acme Conciergerie", min_length = 1, max_length = 200)]
    pub organization_name: String,
}

// ---- Login -----------------------------------------------------------------

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LoginRequest {
    #[schema(example = "owner@example.com")]
    pub email: String,
    #[schema(format = "password")]
    pub password: String,
}

// ---- /me -------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MeResponse {
    pub session: SessionMeta,
    pub user_id: UserId,
    pub memberships: Vec<MembershipSummary>,
    pub is_platform_admin: bool,
    /// When the current session's idle window was last refreshed. Mostly
    /// informational for the frontend to decide when to poll.
    pub resolved_at: DateTime<Utc>,
}
