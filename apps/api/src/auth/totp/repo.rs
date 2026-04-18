//! SQLx persistence for TOTP enrollments and recovery codes.
//!
//! Mirrors the `apps/api/src/auth/repo.rs` style: compile-time-checked
//! queries, one struct per row shape, `AuthError` translation at the
//! repo boundary. Transactions group multi-statement mutations that
//! must stay atomic (pending → confirmed, disable).

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};

use crate::auth::{
    domain::{PasswordHash, UserId},
    error::AuthError,
};

/// Row from `user_totp`.
#[derive(Debug, Clone)]
pub struct TotpRow {
    pub user_id: UserId,
    pub secret_cipher: Vec<u8>,
    pub enrolled_at: Option<DateTime<Utc>>,
    pub disabled_at: Option<DateTime<Utc>>,
}

/// One `user_totp_recovery_codes` row exposed to the service. Only unused
/// rows ever leak out — `list_unused_recovery_codes` filters at the SQL
/// level via the partial index.
#[derive(Debug, Clone)]
pub struct RecoveryCodeRow {
    pub id: i64,
    pub code_hash: PasswordHash,
}

#[derive(Clone)]
pub struct TotpRepo {
    pool: PgPool,
}

impl TotpRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Upsert a pending enrollment for `user_id`. If the user already has
    /// an *active* 2FA row, returns [`AuthError::TotpAlreadyEnrolled`]
    /// without mutating anything. If the user has a *pending* row or no
    /// row, its secret is replaced / created.
    ///
    /// Wrapped in a transaction with `SELECT ... FOR UPDATE` so two
    /// concurrent start-enrollments cannot both "win".
    pub async fn upsert_pending_enrollment(
        &self,
        user_id: UserId,
        secret_cipher: &[u8],
    ) -> Result<(), AuthError> {
        let mut tx: Transaction<'_, Postgres> = self.pool.begin().await?;

        let existing = sqlx::query!(
            r#"
            SELECT enrolled_at, disabled_at
              FROM user_totp
             WHERE user_id = $1
             FOR UPDATE
            "#,
            user_id.into_inner(),
        )
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(row) = existing
            && row.enrolled_at.is_some()
            && row.disabled_at.is_none()
        {
            return Err(AuthError::TotpAlreadyEnrolled);
        }

        sqlx::query!(
            r#"
            INSERT INTO user_totp (user_id, secret_cipher)
            VALUES ($1, $2)
            ON CONFLICT (user_id) DO UPDATE
               SET secret_cipher = EXCLUDED.secret_cipher,
                   enrolled_at   = NULL,
                   disabled_at   = NULL,
                   updated_at    = now()
            "#,
            user_id.into_inner(),
            secret_cipher,
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    /// Return the pending enrollment's ciphertext if one exists for
    /// `user_id`. A row with `enrolled_at IS NOT NULL` counts as *active*,
    /// not pending, and is reported as `None`.
    pub async fn fetch_pending_secret(
        &self,
        user_id: UserId,
    ) -> Result<Option<Vec<u8>>, AuthError> {
        let row = sqlx::query!(
            r#"
            SELECT secret_cipher
              FROM user_totp
             WHERE user_id = $1
               AND enrolled_at IS NULL
            "#,
            user_id.into_inner(),
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.secret_cipher))
    }

    /// Return the active enrollment's ciphertext, or `None` if the user
    /// has no active 2FA (either never enrolled, or disabled).
    pub async fn fetch_active_secret(&self, user_id: UserId) -> Result<Option<Vec<u8>>, AuthError> {
        let row = sqlx::query!(
            r#"
            SELECT secret_cipher
              FROM user_totp
             WHERE user_id = $1
               AND enrolled_at IS NOT NULL
               AND disabled_at IS NULL
            "#,
            user_id.into_inner(),
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.secret_cipher))
    }

    /// Promote a pending enrollment to active and persist the newly
    /// generated recovery-code hashes. Wrapped in one transaction so
    /// partial state (enrolled but no codes, or codes for a still-pending
    /// row) is impossible.
    ///
    /// Returns [`AuthError::TotpNotEnrolled`] if no pending row exists.
    pub async fn confirm_enrollment(
        &self,
        user_id: UserId,
        recovery_hashes: &[PasswordHash],
    ) -> Result<(), AuthError> {
        let mut tx: Transaction<'_, Postgres> = self.pool.begin().await?;

        let rows = sqlx::query!(
            r#"
            UPDATE user_totp
               SET enrolled_at = now(),
                   updated_at  = now()
             WHERE user_id = $1
               AND enrolled_at IS NULL
            "#,
            user_id.into_inner(),
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();

        if rows == 0 {
            return Err(AuthError::TotpNotEnrolled);
        }

        // Wipe any stale recovery codes from a prior enrollment so the
        // caller's newly rendered batch is the only live set.
        sqlx::query!(
            r#"DELETE FROM user_totp_recovery_codes WHERE user_id = $1"#,
            user_id.into_inner(),
        )
        .execute(&mut *tx)
        .await?;

        for hash in recovery_hashes {
            sqlx::query!(
                r#"
                INSERT INTO user_totp_recovery_codes (user_id, code_hash)
                VALUES ($1, $2)
                "#,
                user_id.into_inner(),
                hash.as_db_str(),
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// Rows from `user_totp_recovery_codes` that have not been used yet.
    /// Hits the `idx_user_totp_recovery_codes_unused` partial index.
    pub async fn list_unused_recovery_codes(
        &self,
        user_id: UserId,
    ) -> Result<Vec<RecoveryCodeRow>, AuthError> {
        let rows = sqlx::query!(
            r#"
            SELECT id, code_hash
              FROM user_totp_recovery_codes
             WHERE user_id = $1
               AND used_at IS NULL
             ORDER BY id
            "#,
            user_id.into_inner(),
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| RecoveryCodeRow {
                id: r.id,
                code_hash: PasswordHash::new_unchecked(r.code_hash),
            })
            .collect())
    }

    /// Mark the recovery code identified by `id` as used. Returns `true`
    /// if the row was updated, `false` if it was already consumed (the
    /// atomic single-row update is what races safe: two concurrent
    /// attempts to consume the same code lose exactly one).
    pub async fn mark_recovery_code_used(&self, id: i64) -> Result<bool, AuthError> {
        let rows = sqlx::query!(
            r#"
            UPDATE user_totp_recovery_codes
               SET used_at = now()
             WHERE id = $1
               AND used_at IS NULL
            "#,
            id,
        )
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(rows == 1)
    }

    /// Remove a user's 2FA enrollment and any remaining recovery codes.
    /// Used by `disable`. Safe to call when nothing is enrolled — a
    /// no-op in that case.
    pub async fn delete_enrollment(&self, user_id: UserId) -> Result<(), AuthError> {
        let mut tx: Transaction<'_, Postgres> = self.pool.begin().await?;

        sqlx::query!(
            r#"DELETE FROM user_totp_recovery_codes WHERE user_id = $1"#,
            user_id.into_inner(),
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query!(
            r#"DELETE FROM user_totp WHERE user_id = $1"#,
            user_id.into_inner(),
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    /// Cheap existence check used by `/auth/me` to surface
    /// `mfa_enrolled`. Returns `true` only for active enrollments.
    pub async fn is_enrolled(&self, user_id: UserId) -> Result<bool, AuthError> {
        let row = sqlx::query_scalar!(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM user_totp
                 WHERE user_id = $1
                   AND enrolled_at IS NOT NULL
                   AND disabled_at IS NULL
            ) AS "exists!"
            "#,
            user_id.into_inner(),
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }
}
