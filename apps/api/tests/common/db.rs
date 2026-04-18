//! Ephemeral Postgres harness for integration tests.
//!
//! Spins up a single-use Postgres container via `testcontainers-modules`,
//! runs every migration from `apps/api/migrations/` against it, and hands
//! back a live [`PgPool`]. CLAUDE.md §4.2 forbids mocking the DB; this is
//! the sanctioned alternative.
//!
//! The container is bound to the `TestDatabase` value: drop the value to
//! shut the container down. Tests therefore never leak resources between
//! runs.

#![allow(dead_code, clippy::expect_used, clippy::unwrap_used)] // test harness

use sqlx::{PgPool, postgres::PgPoolOptions};
use testcontainers::{ContainerAsync, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;

/// Handle to a running test database. Keeping this value alive keeps the
/// container running; dropping it stops it.
pub struct TestDatabase {
    _container: ContainerAsync<Postgres>,
    pub pool: PgPool,
    pub url: String,
}

impl TestDatabase {
    /// Boot a new Postgres container, connect, and apply every migration.
    pub async fn spawn() -> Self {
        let container = Postgres::default()
            .start()
            .await
            .expect("start postgres container");

        let host = container.get_host().await.expect("container host");
        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("container port mapping");
        let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await
            .expect("connect to ephemeral postgres");

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("apply migrations to ephemeral postgres");

        Self {
            _container: container,
            pool,
            url,
        }
    }
}
