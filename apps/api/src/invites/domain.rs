//! Invite domain types.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::auth::{Email, OrgId, Role, UserId};

/// Identifier of an invite row. UUIDv4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type, ToSchema)]
#[serde(transparent)]
#[sqlx(transparent)]
#[schema(value_type = String, format = Uuid, example = "00000000-0000-4000-8000-000000000000")]
pub struct InviteId(pub Uuid);

impl InviteId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    #[must_use]
    pub fn into_inner(self) -> Uuid {
        self.0
    }
}

impl Default for InviteId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for InviteId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl From<Uuid> for InviteId {
    fn from(u: Uuid) -> Self {
        Self(u)
    }
}

/// A row from `organization_invites` returned to the service. The
/// `token_hash` stays inside the repo — service / routes never see
/// it. `email` stays an [`Email`] so case-sensitivity and shape are
/// enforced at the type level.
#[derive(Debug, Clone)]
pub struct Invite {
    pub id: InviteId,
    pub org_id: OrgId,
    pub email: Email,
    pub role: Role,
    pub invited_by: UserId,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

// The verify flow previously carried the stored hash out of the repo
// for a defence-in-depth equality check; the SQL `WHERE token_hash =
// $1` predicate already enforces the match, so we return the invite
// alone and keep the hash inside the repo.

/// Service-layer input for creating a new invite.
#[derive(Debug, Clone)]
pub struct CreateInviteInput {
    pub email: Email,
    pub role: Role,
}

/// Preview body returned by `POST /auth/invites/preview` so the UI
/// can render "You're being invited as Cleaner at Acme" without
/// exposing the token or mutating anything.
#[derive(Debug, Clone)]
pub struct InvitePreview {
    pub email: Email,
    pub org_name: String,
    pub role: Role,
    pub expires_at: DateTime<Utc>,
}
