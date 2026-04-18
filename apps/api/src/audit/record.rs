//! Hash-chained audit event insertion.

use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};

use crate::auth::{OrgId, UserId, error::AuthError};

/// Arbitrary 64-bit key for `pg_advisory_xact_lock`. Chosen outside the
/// ranges typically used by extensions / other parts of the schema so it
/// never collides. Keep this constant stable — changing it means past
/// writers may race with new ones.
const AUDIT_ADVISORY_LOCK: i64 = 0x0001_ec0c_c1ea_6e05;

/// An audit event about to be persisted. Construct, then hand to
/// [`AuditRepo::record`].
#[derive(Debug, Clone)]
pub struct AuditEvent {
    /// Short stable identifier (e.g. `"auth.login.success"`). ≤ 64 chars,
    /// snake-case. Enforced by the SQL check constraint.
    pub kind: &'static str,
    /// User the event is *about*. None for anonymous flows (failed login
    /// where we cannot reveal whether the email exists, IP-level bans).
    pub actor_user_id: Option<UserId>,
    /// Organization the event applies to, if any.
    pub org_id: Option<OrgId>,
    /// Structured, JSON-serialisable payload. Callers **must** mask PII
    /// per CLAUDE.md §3.3 — no emails, IBAN, card data, or cleartext
    /// secrets.
    pub payload: serde_json::Value,
}

/// Persistence gateway for the audit log. Clone freely — carries an `Arc`
/// pool internally.
#[derive(Clone)]
pub struct AuditRepo {
    pool: PgPool,
}

impl AuditRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Append an event to the log *outside* of any caller transaction.
    /// Convenience for fire-and-forget events where the caller has no
    /// open transaction (e.g. a failed-login response that otherwise
    /// only reads).
    pub async fn record(&self, event: AuditEvent) -> Result<(), AuthError> {
        let mut tx = self.pool.begin().await?;
        record_in_tx(&mut tx, event).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Append an event to the log **inside** a caller-owned transaction.
    /// Use this whenever the event and the business mutation must be
    /// atomic (signup writes the user *and* the `auth.signup` event in
    /// the same commit).
    pub async fn record_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        event: AuditEvent,
    ) -> Result<(), AuthError> {
        record_in_tx(tx, event).await
    }
}

async fn record_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    event: AuditEvent,
) -> Result<(), AuthError> {
    // Serialise concurrent inserts so the hash chain stays contiguous.
    // Released automatically at commit / rollback.
    sqlx::query!("SELECT pg_advisory_xact_lock($1)", AUDIT_ADVISORY_LOCK)
        .execute(&mut **tx)
        .await?;

    let prev_hash: Option<Vec<u8>> =
        sqlx::query_scalar!(r#"SELECT hash FROM audit_events ORDER BY id DESC LIMIT 1"#)
            .fetch_optional(&mut **tx)
            .await?;

    let canonical = canonicalize(&event);
    let hash = chain_hash(prev_hash.as_deref(), &canonical);

    let actor = event.actor_user_id.map(UserId::into_inner);
    let org = event.org_id.map(OrgId::into_inner);

    sqlx::query!(
        r#"
        INSERT INTO audit_events (kind, actor_user_id, org_id, payload, prev_hash, hash)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
        event.kind,
        actor,
        org,
        event.payload,
        prev_hash,
        hash.as_slice(),
    )
    .execute(&mut **tx)
    .await?;

    Ok(())
}

/// Deterministic byte representation of the event used as input to the
/// hash function. Keep this format stable — changing it invalidates every
/// chain verification from that point onwards.
///
/// Fields separated by `0x1E` (ASCII record separator) so boundaries are
/// unambiguous regardless of their contents.
fn canonicalize(event: &AuditEvent) -> Vec<u8> {
    const SEP: u8 = 0x1E;
    let mut out = Vec::with_capacity(128);
    out.extend_from_slice(event.kind.as_bytes());
    out.push(SEP);
    if let Some(u) = event.actor_user_id {
        out.extend_from_slice(u.into_inner().as_bytes());
    }
    out.push(SEP);
    if let Some(o) = event.org_id {
        out.extend_from_slice(o.into_inner().as_bytes());
    }
    out.push(SEP);
    // `serde_json::to_vec` is deterministic for the same `Value` tree
    // because `Value` maps preserve insertion order in serde_json.
    if let Ok(json) = serde_json::to_vec(&event.payload) {
        out.extend_from_slice(&json);
    }
    out
}

fn chain_hash(prev: Option<&[u8]>, canonical: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    if let Some(p) = prev {
        hasher.update(p);
    }
    hasher.update(canonical);
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_is_deterministic_for_same_input() {
        let a = AuditEvent {
            kind: "auth.login.success",
            actor_user_id: None,
            org_id: None,
            payload: serde_json::json!({ "ip": "a***" }),
        };
        let b = a.clone();
        assert_eq!(canonicalize(&a), canonicalize(&b));
    }

    #[test]
    fn chain_hash_depends_on_prev() {
        let canonical = b"x";
        let h1 = chain_hash(None, canonical);
        let h2 = chain_hash(Some(&[0u8; 32]), canonical);
        assert_ne!(h1, h2);
    }

    #[test]
    fn chain_hash_depends_on_payload() {
        let h1 = chain_hash(None, b"x");
        let h2 = chain_hash(None, b"y");
        assert_ne!(h1, h2);
    }
}
