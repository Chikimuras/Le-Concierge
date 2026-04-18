//! SQLx-backed persistence layer for the `auth` domain.
//!
//! Every query uses the compile-time-checked `sqlx::query!` /
//! `sqlx::query_as!` macros (CLAUDE.md §3.4 / §7.1): this guarantees SQL
//! type/shape mismatches become build errors rather than runtime panics.
//!
//! Offline builds (CI without a live DB) rely on `.sqlx/` prepared files —
//! run `cargo sqlx prepare --workspace` after changing any query.

use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::auth::{
    domain::{Email, OrgId, PasswordHash, Slug, UserId},
    error::AuthError,
};

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
