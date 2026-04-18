//! SQLx persistence for team invites. Every query is scoped by
//! `org_id` when called from a tenant-scoped handler; the accept /
//! signup flow (anonymous) uses `find_active_by_token_hash` which is
//! the only cross-tenant access point — protected by the
//! unguessable 256-bit token.

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};

use crate::{
    auth::{Email, OrgId, Role, UserId},
    invites::{
        domain::{CreateInviteInput, Invite, InviteId},
        error::InviteError,
    },
};

#[derive(Clone)]
pub struct InviteRepo {
    pool: PgPool,
}

impl InviteRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Insert a new pending invite. The partial unique on
    /// `(org_id, email)` surfaces as [`InviteError::AlreadyPending`];
    /// any other DB error bubbles up as `Repository`.
    pub async fn create(
        &self,
        org_id: OrgId,
        invited_by: UserId,
        input: &CreateInviteInput,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<Invite, InviteError> {
        let r = sqlx::query!(
            r#"
            INSERT INTO organization_invites
                (org_id, email, role, invited_by, token_hash, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, org_id, email AS "email: Email",
                      role AS "role: Role", invited_by,
                      expires_at, created_at
            "#,
            org_id.into_inner(),
            input.email.as_str(),
            input.role as Role,
            invited_by.into_inner(),
            token_hash,
            expires_at,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(translate_unique_violation)?;

        Ok(Invite {
            id: InviteId::from(r.id),
            org_id: OrgId::from(r.org_id),
            email: r.email,
            role: r.role,
            invited_by: UserId::from(r.invited_by),
            expires_at: r.expires_at,
            created_at: r.created_at,
        })
    }

    /// List pending (non-cancelled, non-accepted, non-expired) invites
    /// for `org_id`. Hits the partial index.
    pub async fn list_pending(&self, org_id: OrgId) -> Result<Vec<Invite>, InviteError> {
        let rows = sqlx::query!(
            r#"
            SELECT id, org_id, email AS "email: Email",
                   role AS "role: Role", invited_by,
                   expires_at, created_at
              FROM organization_invites
             WHERE org_id = $1
               AND accepted_at IS NULL
               AND cancelled_at IS NULL
               AND expires_at > now()
             ORDER BY created_at DESC
            "#,
            org_id.into_inner(),
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| Invite {
                id: InviteId::from(r.id),
                org_id: OrgId::from(r.org_id),
                email: r.email,
                role: r.role,
                invited_by: UserId::from(r.invited_by),
                expires_at: r.expires_at,
                created_at: r.created_at,
            })
            .collect())
    }

    /// Cancel a pending invite scoped by `org_id`. Returns `NotFound`
    /// when no active row matches — covers "wrong org", "already
    /// cancelled/accepted", and "never existed" uniformly.
    pub async fn cancel(&self, org_id: OrgId, id: InviteId) -> Result<(), InviteError> {
        let rows = sqlx::query!(
            r#"
            UPDATE organization_invites
               SET cancelled_at = now()
             WHERE id = $1
               AND org_id = $2
               AND accepted_at IS NULL
               AND cancelled_at IS NULL
            "#,
            id.into_inner(),
            org_id.into_inner(),
        )
        .execute(&self.pool)
        .await?
        .rows_affected();
        if rows == 0 {
            Err(InviteError::NotFound)
        } else {
            Ok(())
        }
    }

    /// Resolve an invite by its hashed token. Returns `NotFound` when
    /// no active row matches (unknown / cancelled / already
    /// accepted), `Expired` when the row is pending but past its
    /// TTL. The token itself never reaches this method — the service
    /// layer hashes first.
    pub async fn find_active_by_token_hash(&self, token_hash: &str) -> Result<Invite, InviteError> {
        let r = sqlx::query!(
            r#"
            SELECT id, org_id, email AS "email: Email",
                   role AS "role: Role", invited_by,
                   expires_at, created_at
              FROM organization_invites
             WHERE token_hash = $1
               AND accepted_at IS NULL
               AND cancelled_at IS NULL
            "#,
            token_hash,
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(InviteError::NotFound)?;

        if r.expires_at <= Utc::now() {
            return Err(InviteError::Expired);
        }

        Ok(Invite {
            id: InviteId::from(r.id),
            org_id: OrgId::from(r.org_id),
            email: r.email,
            role: r.role,
            invited_by: UserId::from(r.invited_by),
            expires_at: r.expires_at,
            created_at: r.created_at,
        })
    }

    /// Consume an invite + link the user to the org as a membership,
    /// atomically. Returns `NotFound` if someone else consumed the row
    /// in-between lookup and update (lost race). Caller passes the
    /// `accepting_user_id` — identity checks (email match, new-vs-
    /// existing) happen in the service layer.
    pub async fn accept(
        &self,
        invite_id: InviteId,
        accepting_user: UserId,
        org_id: OrgId,
        role: Role,
    ) -> Result<(), InviteError> {
        let mut tx: Transaction<'_, Postgres> = self.pool.begin().await?;

        let consumed = sqlx::query!(
            r#"
            UPDATE organization_invites
               SET accepted_at = now(),
                   accepted_by = $2
             WHERE id = $1
               AND accepted_at IS NULL
               AND cancelled_at IS NULL
               AND expires_at > now()
            "#,
            invite_id.into_inner(),
            accepting_user.into_inner(),
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();

        if consumed == 0 {
            return Err(InviteError::NotFound);
        }

        // Upsert the membership: if the user already has one (e.g.
        // accepted twice in parallel), keep the existing row untouched.
        sqlx::query!(
            r#"
            INSERT INTO organization_members (user_id, org_id, role, invited_by)
            SELECT $1, $2, $3, invited_by
              FROM organization_invites
             WHERE id = $4
            ON CONFLICT (user_id, org_id) DO NOTHING
            "#,
            accepting_user.into_inner(),
            org_id.into_inner(),
            role as Role,
            invite_id.into_inner(),
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }
}

fn translate_unique_violation(err: sqlx::Error) -> InviteError {
    if let sqlx::Error::Database(db_err) = &err
        && db_err.code().as_deref() == Some("23505")
    {
        return InviteError::AlreadyPending;
    }
    InviteError::Repository(err)
}
