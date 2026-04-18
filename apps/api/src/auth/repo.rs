//! SQLx-backed persistence layer for the `auth` domain.
//!
//! Every query uses the compile-time-checked `sqlx::query!` /
//! `sqlx::query_as!` macros (CLAUDE.md §3.4 / §7.1): this guarantees SQL
//! type/shape mismatches become build errors rather than runtime panics.
//!
//! Offline builds (CI without a live DB) rely on `.sqlx/` prepared files —
//! run `cargo sqlx prepare --workspace` after changing any query.

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::auth::{
    domain::{Email, OrgId, PasswordHash, Role, Slug, UserId},
    error::AuthError,
};

/// Row returned by [`AuthRepo::find_user_by_email`].
#[derive(Debug, Clone)]
pub struct UserRow {
    pub id: UserId,
    pub password_hash: PasswordHash,
    pub failed_login_attempts: i32,
    pub locked_until: Option<DateTime<Utc>>,
}

/// Lightweight row returned by [`AuthRepo::find_user_by_id`] — enough
/// for the TOTP flows that need the email (for `otpauth://`) and the
/// password hash (for re-verification on disable).
#[derive(Debug, Clone)]
pub struct UserIdRow {
    pub email: Email,
    pub password_hash: PasswordHash,
}

/// Row describing a user's role inside an organization. Returned by
/// [`AuthRepo::list_memberships`].
#[derive(Debug, Clone)]
pub struct MembershipRow {
    pub org_id: OrgId,
    pub org_slug: String,
    pub org_name: String,
    pub role: Role,
}

/// Persistence gateway. Holds an owned `PgPool`; cheap to clone (it's an
/// `Arc` internally).
#[derive(Clone)]
pub struct AuthRepo {
    pool: PgPool,
}

impl AuthRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Atomically create an organization, the first user, and the membership
    /// linking them as `owner`.
    ///
    /// Runs inside a single transaction: if any step fails, nothing is
    /// persisted. Unique-constraint violations on email or slug are
    /// translated into the corresponding [`AuthError`] variants so callers
    /// can render a friendly message.
    ///
    /// # Errors
    ///
    /// - [`AuthError::EmailAlreadyTaken`] when the email is already in use.
    /// - [`AuthError::SlugAlreadyTaken`] when the slug collides.
    /// - [`AuthError::Repository`] for every other SQLx error.
    pub async fn create_organization_with_owner(
        &self,
        email: &Email,
        password_hash: &PasswordHash,
        slug: &Slug,
        name: &str,
    ) -> Result<(OrgId, UserId), AuthError> {
        let mut tx: Transaction<'_, Postgres> = self.pool.begin().await?;

        let org_id: Uuid = sqlx::query_scalar!(
            r#"
            INSERT INTO organizations (slug, name)
            VALUES ($1, $2)
            RETURNING id
            "#,
            slug.as_str(),
            name,
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(translate_unique_violation)?;

        let user_id: Uuid = sqlx::query_scalar!(
            r#"
            INSERT INTO users (email, password_hash)
            VALUES ($1, $2)
            RETURNING id
            "#,
            email.as_str(),
            password_hash.as_db_str(),
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(translate_unique_violation)?;

        sqlx::query!(
            r#"
            INSERT INTO organization_members (user_id, org_id, role)
            VALUES ($1, $2, 'owner'::role)
            "#,
            user_id,
            org_id,
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok((OrgId::from(org_id), UserId::from(user_id)))
    }

    /// Look up a user by email (case-insensitive via `citext`). Returns
    /// `Ok(None)` if no such user exists.
    pub async fn find_user_by_email(&self, email: &Email) -> Result<Option<UserRow>, AuthError> {
        let row = sqlx::query!(
            r#"
            SELECT id,
                   password_hash,
                   failed_login_attempts,
                   locked_until
            FROM users
            WHERE email = $1
            "#,
            email.as_str(),
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| UserRow {
            id: UserId::from(r.id),
            password_hash: PasswordHash::new_unchecked(r.password_hash),
            failed_login_attempts: r.failed_login_attempts,
            locked_until: r.locked_until,
        }))
    }

    /// Increment the failure counter and apply a progressive lockout when
    /// the user has failed too many consecutive attempts. Returns the new
    /// lockout deadline if one was set, or `None` if the user is still
    /// within the grace window.
    ///
    /// Window schedule (OWASP ASVS 2.2.1 — progressive):
    ///
    /// | failures | lockout   |
    /// |----------|-----------|
    /// | 1-4      | none      |
    /// | 5        | 10 min    |
    /// | 6-9      | 1 h       |
    /// | 10-19    | 1 day     |
    /// | ≥ 20     | 7 days    |
    pub async fn record_failed_login(
        &self,
        user_id: UserId,
    ) -> Result<Option<DateTime<Utc>>, AuthError> {
        let new_count: i32 = sqlx::query_scalar!(
            r#"
            UPDATE users
               SET failed_login_attempts = failed_login_attempts + 1
             WHERE id = $1
         RETURNING failed_login_attempts
            "#,
            user_id.into_inner(),
        )
        .fetch_one(&self.pool)
        .await?;

        let Some(window) = progressive_lockout_window(new_count) else {
            return Ok(None);
        };

        let until = Utc::now() + window;
        sqlx::query!(
            r#"
            UPDATE users
               SET locked_until = $2
             WHERE id = $1
            "#,
            user_id.into_inner(),
            until,
        )
        .execute(&self.pool)
        .await?;

        Ok(Some(until))
    }

    /// Check whether a user is a platform admin.
    pub async fn is_platform_admin(&self, user_id: UserId) -> Result<bool, AuthError> {
        let row = sqlx::query_scalar!(
            r#"SELECT 1 AS "exists!" FROM platform_admins WHERE user_id = $1"#,
            user_id.into_inner(),
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.is_some())
    }

    /// List every organization the user belongs to with the associated role.
    pub async fn list_memberships(&self, user_id: UserId) -> Result<Vec<MembershipRow>, AuthError> {
        let rows = sqlx::query!(
            r#"
            SELECT o.id   AS org_id,
                   o.slug AS org_slug,
                   o.name AS org_name,
                   om.role AS "role: Role"
            FROM organization_members om
            JOIN organizations o ON o.id = om.org_id
            WHERE om.user_id = $1
            ORDER BY o.created_at ASC
            "#,
            user_id.into_inner(),
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| MembershipRow {
                org_id: OrgId::from(r.org_id),
                org_slug: r.org_slug,
                org_name: r.org_name,
                role: r.role,
            })
            .collect())
    }

    /// Fetch the email + password hash for `user_id`. Used by the TOTP
    /// flow (`otpauth://` URL needs the email; disable re-checks the
    /// password). Returns `None` if the user no longer exists.
    pub async fn find_user_by_id(&self, user_id: UserId) -> Result<Option<UserIdRow>, AuthError> {
        let row = sqlx::query!(
            r#"
            SELECT email         AS "email: Email",
                   password_hash
              FROM users
             WHERE id = $1
            "#,
            user_id.into_inner(),
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| UserIdRow {
            email: r.email,
            password_hash: PasswordHash::new_unchecked(r.password_hash),
        }))
    }

    /// Clear the failure counter and lockout on successful authentication.
    pub async fn reset_failed_logins(&self, user_id: UserId) -> Result<(), AuthError> {
        sqlx::query!(
            r#"
            UPDATE users
               SET failed_login_attempts = 0,
                   locked_until = NULL
             WHERE id = $1
            "#,
            user_id.into_inner(),
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

/// Progressive lockout schedule. Returns `None` for counts that should not
/// trigger a lockout.
fn progressive_lockout_window(attempts: i32) -> Option<chrono::Duration> {
    match attempts {
        ..=4 => None,
        5..=9 => Some(chrono::Duration::minutes(10)),
        10..=19 => Some(chrono::Duration::hours(1)),
        20..=49 => Some(chrono::Duration::days(1)),
        _ => Some(chrono::Duration::days(7)),
    }
}

/// Map Postgres unique-violation (`SQLSTATE 23505`) to a friendlier domain
/// error when the constraint name hints at which column collided. Any other
/// database error falls through to [`AuthError::Repository`].
fn translate_unique_violation(err: sqlx::Error) -> AuthError {
    if let sqlx::Error::Database(db_err) = &err
        && db_err.code().as_deref() == Some("23505")
        && let Some(constraint) = db_err.constraint()
    {
        if constraint.contains("email") {
            return AuthError::EmailAlreadyTaken;
        }
        if constraint.contains("slug") {
            return AuthError::SlugAlreadyTaken;
        }
    }
    AuthError::Repository(err)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)] // test assertions
mod tests {
    use super::*;

    #[test]
    fn progressive_window_is_monotonic() {
        assert!(progressive_lockout_window(0).is_none());
        assert!(progressive_lockout_window(4).is_none());
        let a = progressive_lockout_window(5).unwrap();
        let b = progressive_lockout_window(10).unwrap();
        let c = progressive_lockout_window(20).unwrap();
        let d = progressive_lockout_window(200).unwrap();
        assert!(a < b && b < c && c < d);
    }
}
