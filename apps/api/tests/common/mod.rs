//! Shared harness for integration tests.
//!
//! Each `#[tokio::test]` that needs the real HTTP server calls
//! [`spawn_app`] to obtain an isolated instance listening on a random
//! loopback port. The database pool is lazy, so tests that do not touch
//! the DB (such as those for `/healthz`) run without requiring Postgres.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)] // test harness

pub mod db;

use api::{
    AppState, Config, build_app,
    config::{AuthConfig, CorsConfig, DatabaseConfig, HttpConfig, LogFormat, TelemetryConfig},
};
use secrecy::SecretString;

pub struct TestApp {
    pub base_url: String,
}

impl TestApp {
    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}

/// Build a config suitable for tests. Uses a deliberately invalid DB URL
/// that the lazy pool never attempts to connect to as long as no handler
/// issues a query.
fn test_config() -> Config {
    Config {
        http: HttpConfig {
            bind: "127.0.0.1:0".parse().expect("valid test bind"),
            request_timeout_secs: 10,
            public_base_url: "http://localhost".into(),
        },
        database: DatabaseConfig {
            url: "postgres://test:test@127.0.0.1:54329/test".into(),
            max_connections: 1,
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
    }
}

/// Spin up the real app on an OS-assigned loopback port and return its base URL.
pub async fn spawn_app() -> TestApp {
    let state = AppState::new(test_config()).expect("AppState::new");
    let app = build_app(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind loopback");
    let addr = listener.local_addr().expect("local_addr");

    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    TestApp {
        base_url: format!("http://{addr}"),
    }
}
