//! Shared application state injected into every handler via `State<AppState>`.
//!
//! `AppState` is cheap to clone: every field is reference-counted internally
//! (`Arc` for [`Config`], the `PgPool` clone shares its `Arc`-wrapped inner,
//! the Redis `ConnectionManager` inside `SessionService` is `Arc`-backed,
//! and [`AuthService`] composes those). Handlers never mutate state
//! directly — they read config and call service methods.
//!
//! The `PgPool` is constructed **lazily** (`connect_lazy`): no TCP connection
//! is established at boot. The Redis connection manager, by contrast, is
//! eagerly established because the `redis` crate does not offer a lazy
//! equivalent — which is why [`AppState::new`] is async.

use std::{sync::Arc, time::Duration};

use sqlx::{PgPool, postgres::PgPoolOptions};

use crate::{
    audit::AuditRepo,
    auth::{AuthRepo, AuthService},
    config::Config,
    session::{RedisSessionStore, SessionService, cookie::CookieConfig},
};

/// Root application state.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: PgPool,
    pub session: SessionService,
    pub auth: AuthService,
}

impl AppState {
    /// Build state from a loaded [`Config`]. Establishes the Redis
    /// connection (eager) but not the Postgres pool (lazy).
    ///
    /// # Errors
    ///
    /// Returns an error if the configured database URL fails to parse,
    /// or if the Redis connection cannot be established within the
    /// default handshake window.
    pub async fn new(config: Config) -> anyhow::Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(config.database.max_connections)
            .min_connections(config.database.min_connections)
            .acquire_timeout(Duration::from_secs(config.database.statement_timeout_secs))
            .connect_lazy(&config.database.url)
            .map_err(|e| anyhow::anyhow!("invalid database url: {e}"))?;

        let redis_store = RedisSessionStore::connect(&config.redis.url)
            .await
            .map_err(|e| anyhow::anyhow!("redis connect failed: {e}"))?;
        let session = SessionService::new(
            redis_store,
            Duration::from_secs(config.session.idle_ttl_secs),
            Duration::from_secs(config.session.absolute_ttl_secs),
        );

        let auth_repo = AuthRepo::new(pool.clone());
        let audit_repo = AuditRepo::new(pool.clone());
        let auth = AuthService::new(
            auth_repo,
            audit_repo,
            session.clone(),
            config.auth.pepper.clone(),
        );

        Ok(Self {
            config: Arc::new(config),
            db: pool,
            session,
            auth,
        })
    }

    /// Build state from a set of pre-built dependencies. Used by the
    /// integration test harness (and by anything else that wants to wire
    /// a custom session store). Kept public rather than feature-gated so
    /// tests do not need a dedicated Cargo feature.
    #[must_use]
    pub fn from_parts(config: Config, pool: PgPool, session: SessionService) -> Self {
        let auth_repo = AuthRepo::new(pool.clone());
        let audit_repo = AuditRepo::new(pool.clone());
        let auth = AuthService::new(
            auth_repo,
            audit_repo,
            session.clone(),
            config.auth.pepper.clone(),
        );
        Self {
            config: Arc::new(config),
            db: pool,
            session,
            auth,
        }
    }

    /// Convenience accessor for the idle TTL, used by cookie builders.
    #[must_use]
    pub fn session_idle_ttl(&self) -> Duration {
        self.session.idle_ttl()
    }

    /// Build the [`CookieConfig`] the cookie helpers expect.
    #[must_use]
    pub fn cookie_config(&self) -> CookieConfig {
        CookieConfig {
            secure: self.config.session.cookie_secure,
            domain: self.config.session.cookie_domain.clone(),
        }
    }
}
