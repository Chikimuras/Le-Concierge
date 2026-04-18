//! Shared harness for integration tests.
//!
//! Each `#[tokio::test]` that needs the real HTTP server calls
//! [`spawn_app`] to obtain an isolated instance listening on a random
//! loopback port. Postgres and Redis both run in per-suite
//! testcontainers — CLAUDE.md §4.2 forbids mocking either.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)] // test harness

pub mod db;
pub mod redis;

use std::{net::SocketAddr, time::Duration};

use api::{
    AppState, Config, build_app,
    config::{
        AuthConfig, CorsConfig, DatabaseConfig, HttpConfig, LogFormat, RedisConfig, SessionConfig,
        TelemetryConfig,
    },
    session::SessionService,
};
use secrecy::SecretString;

use crate::common::{db::TestDatabase, redis::TestRedis};

/// Test handle bundling the live HTTP server plus the ephemeral
/// Postgres and Redis containers it sits on. Dropping the value stops
/// the server and tears the containers down.
pub struct TestApp {
    pub base_url: String,
    pub db: TestDatabase,
    pub redis: TestRedis,
}

impl TestApp {
    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}

/// Build a plausible test config. DB and Redis URLs come from the
/// caller-owned containers; every other value is a safe static default.
fn test_config(db_url: String, redis_url: String) -> Config {
    Config {
        http: HttpConfig {
            bind: "127.0.0.1:0".parse().expect("valid test bind"),
            request_timeout_secs: 30,
            public_base_url: "http://localhost".into(),
        },
        database: DatabaseConfig {
            url: db_url,
            max_connections: 5,
            min_connections: 0,
            statement_timeout_secs: 5,
        },
        telemetry: TelemetryConfig {
            format: LogFormat::Pretty,
            filter: "warn".into(),
            service_name: "api-test".into(),
        },
        cors: CorsConfig {
            allowed_origins: vec![],
        },
        auth: AuthConfig {
            pepper: SecretString::from("dangerous-test-pepper-never-use-in-prod"),
        },
        session: SessionConfig {
            idle_ttl_secs: 600,
            absolute_ttl_secs: 3600,
            cookie_secure: false,
            cookie_domain: None,
        },
        redis: RedisConfig { url: redis_url },
    }
}

/// Spin up Postgres + Redis containers and the real app on an
/// OS-assigned loopback port.
pub async fn spawn_app() -> TestApp {
    let db = TestDatabase::spawn().await;
    let redis = TestRedis::spawn().await;

    let config = test_config(db.url.clone(), redis.url.clone());
    let session = SessionService::new(
        redis.store.clone(),
        Duration::from_secs(config.session.idle_ttl_secs),
        Duration::from_secs(config.session.absolute_ttl_secs),
    );

    let state = AppState::from_parts(config, db.pool.clone(), session).expect("auth state");
    let app = build_app(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind loopback");
    let addr = listener.local_addr().expect("local_addr");

    tokio::spawn(async move {
        let _ = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await;
    });

    TestApp {
        base_url: format!("http://{addr}"),
        db,
        redis,
    }
}
