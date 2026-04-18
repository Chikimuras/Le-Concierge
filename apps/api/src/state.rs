//! Shared application state injected into every handler via `State<AppState>`.
//!
//! `AppState` is cheap to clone: every field is reference-counted internally
//! (`Arc` for [`Config`], the `PgPool` clone shares its `Arc`-wrapped inner,
//! and [`AuthService`] holds `Arc`-backed values). Handlers never mutate
//! state directly — they read config and call service methods.
//!
//! The `PgPool` is constructed **lazily** (`connect_lazy`): no TCP connection
//! is established at boot. The first query determines whether the database
//! is reachable. This keeps `GET /healthz` decoupled from database state and
//! allows integration tests for non-DB routes to run without Postgres.

use std::sync::Arc;

use sqlx::{PgPool, postgres::PgPoolOptions};

use crate::{
    auth::{AuthRepo, AuthService},
    config::Config,
};

/// Root application state. Clone this freely — it's composed of `Arc`s and
/// `Arc`-backed handles.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: PgPool,
    pub auth: AuthService,
}

impl AppState {
    /// Construct state from a loaded [`Config`]. Does **not** open a DB
    /// connection; see module docs.
    ///
    /// # Errors
    ///
    /// Returns an error if the configured database URL fails to parse.
    pub fn new(config: Config) -> anyhow::Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(config.database.max_connections)
            .min_connections(config.database.min_connections)
            .acquire_timeout(std::time::Duration::from_secs(
                config.database.statement_timeout_secs,
            ))
            .connect_lazy(&config.database.url)
            .map_err(|e| anyhow::anyhow!("invalid database url: {e}"))?;

        let auth = AuthService::new(AuthRepo::new(pool.clone()), config.auth.pepper.clone());

        Ok(Self {
            config: Arc::new(config),
            db: pool,
            auth,
        })
    }
}
