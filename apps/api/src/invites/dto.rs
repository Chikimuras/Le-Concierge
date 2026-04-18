//! HTTP request / response shapes for `/orgs/:slug/invites` and
//! `/auth/invites/{preview,accept,signup}`.
//!
//! Every incoming DTO uses `#[serde(deny_unknown_fields)]` so
//! client-drift becomes 422, never silent. No response DTO ever
//! echoes the plaintext token (ADR 0009).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    auth::{OrgId, Role},
    invites::domain::{Invite, InviteId, InvitePreview},
};

// ---- Manager-side --------------------------------------------------------

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateInviteRequest {
    #[schema(example = "teammate@example.com")]
    pub email: String,
    #[schema(example = "manager")]
    pub role: Role,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct InviteResponse {
    pub id: InviteId,
    pub org_id: OrgId,
    #[schema(example = "teammate@example.com")]
    pub email: String,
    pub role: Role,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl From<Invite> for InviteResponse {
    fn from(i: Invite) -> Self {
        Self {
            id: i.id,
            org_id: i.org_id,
            email: i.email.as_str().to_owned(),
            role: i.role,
            expires_at: i.expires_at,
            created_at: i.created_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct InviteListResponse {
    pub invites: Vec<InviteResponse>,
}

// ---- Invitee-side --------------------------------------------------------

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PreviewRequest {
    /// The 43-char URL-safe base64 token delivered by email. Treated
    /// as a secret — never logged server-side.
    pub token: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PreviewResponse {
    #[schema(example = "teammate@example.com")]
    pub email: String,
    #[schema(example = "Acme Conciergerie")]
    pub org_name: String,
    pub role: Role,
    pub expires_at: DateTime<Utc>,
}

impl From<InvitePreview> for PreviewResponse {
    fn from(p: InvitePreview) -> Self {
        Self {
            email: p.email.as_str().to_owned(),
            org_name: p.org_name,
            role: p.role,
            expires_at: p.expires_at,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AcceptRequest {
    pub token: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SignupAndAcceptRequest {
    pub token: String,
    /// Minimum 12 characters (CLAUDE.md §3.1).
    #[schema(format = "password", min_length = 12)]
    pub password: String,
}
