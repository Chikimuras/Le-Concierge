//! Redis-backed session store.
//!
//! Sessions live under `session:<encoded-id>` keys. Each write sets the
//! *idle* TTL so inactive sessions evict automatically; the absolute
//! deadline is enforced in [`super::service::SessionService`] on every
//! lookup.

use std::time::Duration;

use redis::{AsyncCommands, aio::ConnectionManager};

use crate::session::{
    dto::{SessionData, SessionId},
    error::SessionError,
};

/// Thin wrapper over a `redis::aio::ConnectionManager`. Clone cheaply —
/// the manager is already an `Arc` inside.
#[derive(Clone)]
pub struct RedisSessionStore {
    conn: ConnectionManager,
}

impl RedisSessionStore {
    /// Open a connection manager against the configured URL. Fails if
    /// the URL is malformed or the initial handshake fails.
    pub async fn connect(url: &str) -> Result<Self, SessionError> {
        let client = redis::Client::open(url).map_err(|e| SessionError::Backend(e.into()))?;
        let conn = ConnectionManager::new(client)
            .await
            .map_err(|e| SessionError::Backend(e.into()))?;
        Ok(Self { conn })
    }

    /// Pre-built store for callers that already have a `ConnectionManager`
    /// (typically the integration test harness — [`RedisSessionStore::connect`]
    /// already wraps the manager they want). Keeping this public rather
    /// than feature-gated avoids having to declare a `test-helpers`
    /// Cargo feature just to let integration tests reach it.
    #[must_use]
    pub fn from_manager(conn: ConnectionManager) -> Self {
        Self { conn }
    }

    /// Serialize and write the session with an idle TTL.
    pub async fn save(
        &self,
        id: &SessionId,
        data: &SessionData,
        idle_ttl: Duration,
    ) -> Result<(), SessionError> {
        let payload = serde_json::to_vec(data)?;
        let mut conn = self.conn.clone();
        let _: () = conn
            .set_ex(id.redis_key(), payload, idle_ttl.as_secs())
            .await?;
        Ok(())
    }

    /// Fetch the session payload if present. Returns `Ok(None)` if the
    /// key is missing.
    pub async fn load(&self, id: &SessionId) -> Result<Option<SessionData>, SessionError> {
        let mut conn = self.conn.clone();
        let value: Option<Vec<u8>> = conn.get(id.redis_key()).await?;
        match value {
            None => Ok(None),
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        }
    }

    /// Refresh the idle TTL on an existing key. Does nothing if the key
    /// has already expired between a [`load`] and this call — that race
    /// is benign.
    pub async fn touch(&self, id: &SessionId, idle_ttl: Duration) -> Result<(), SessionError> {
        let mut conn = self.conn.clone();
        let _: i64 = conn
            .expire(
                id.redis_key(),
                i64::try_from(idle_ttl.as_secs()).unwrap_or(i64::MAX),
            )
            .await?;
        Ok(())
    }

    /// Remove the session key. Idempotent — deleting a missing key is not
    /// an error.
    pub async fn destroy(&self, id: &SessionId) -> Result<(), SessionError> {
        let mut conn = self.conn.clone();
        let _: i64 = conn.del(id.redis_key()).await?;
        Ok(())
    }
}
