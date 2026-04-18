//! High-level session lifecycle operations.

use std::{net::IpAddr, time::Duration};

use chrono::Utc;

use crate::{
    auth::UserId,
    session::{
        dto::{SessionData, SessionId, fingerprint_user_agent, generate_csrf_token, mask_ip},
        error::SessionError,
        store::RedisSessionStore,
    },
};

/// Thin orchestration over [`RedisSessionStore`] that implements the TTL
/// semantics documented in ADR 0006:
///
/// - **Idle TTL** — refreshed on every successful lookup. Sessions that
///   go unused for longer than `idle_ttl` disappear from Redis
///   automatically (Redis expires the key).
/// - **Absolute TTL** — hard cap computed at creation time. Even a
///   session touched every minute dies after `absolute_ttl`.
#[derive(Clone)]
pub struct SessionService {
    store: RedisSessionStore,
    idle_ttl: Duration,
    absolute_ttl: Duration,
}

impl SessionService {
    #[must_use]
    pub fn new(store: RedisSessionStore, idle_ttl: Duration, absolute_ttl: Duration) -> Self {
        Self {
            store,
            idle_ttl,
            absolute_ttl,
        }
    }

    /// Create a new session for `user_id`. Generates the `SessionId` and
    /// CSRF token, masks / fingerprints the request context, and persists
    /// with the idle TTL.
    pub async fn create(
        &self,
        user_id: UserId,
        ip: IpAddr,
        user_agent: &str,
    ) -> Result<(SessionId, SessionData), SessionError> {
        let id = SessionId::generate();
        let now = Utc::now();
        let absolute_expires_at = now
            + chrono::Duration::from_std(self.absolute_ttl)
                .unwrap_or_else(|_| chrono::Duration::days(30));
        let data = SessionData {
            user_id,
            csrf_token: generate_csrf_token(),
            mfa_verified: false,
            created_at: now,
            absolute_expires_at,
            ip_masked: mask_ip(ip),
            user_agent_fingerprint: fingerprint_user_agent(user_agent),
        };
        self.store.save(&id, &data, self.idle_ttl).await?;
        Ok((id, data))
    }

    /// Fetch the session identified by `id`, enforcing both TTLs. Refreshes
    /// the idle TTL on success so active sessions stay alive.
    ///
    /// Returns [`SessionError::NotFound`] if the key no longer exists in
    /// Redis, or [`SessionError::Expired`] if the absolute cut-off has
    /// been reached (in which case the key is also evicted).
    pub async fn lookup(&self, id: &SessionId) -> Result<SessionData, SessionError> {
        let data = self.store.load(id).await?.ok_or(SessionError::NotFound)?;
        if data.is_absolutely_expired(Utc::now()) {
            // Best-effort cleanup; even if it fails the key has (at most)
            // the remaining idle TTL left before Redis drops it.
            let _ = self.store.destroy(id).await;
            return Err(SessionError::Expired);
        }
        self.store.touch(id, self.idle_ttl).await?;
        Ok(data)
    }

    /// Destroy the session. Idempotent.
    pub async fn destroy(&self, id: &SessionId) -> Result<(), SessionError> {
        self.store.destroy(id).await
    }

    #[must_use]
    pub fn idle_ttl(&self) -> Duration {
        self.idle_ttl
    }

    #[must_use]
    pub fn absolute_ttl(&self) -> Duration {
        self.absolute_ttl
    }
}
