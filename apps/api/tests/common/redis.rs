//! Ephemeral Redis harness for integration tests.
//!
//! Mirrors `tests/common/db.rs`: spins up a throwaway container, returns
//! a ready-to-use [`RedisSessionStore`] plus the container handle that
//! keeps the instance alive for the duration of the test.

#![allow(dead_code, clippy::expect_used, clippy::unwrap_used)] // test harness

use api::session::RedisSessionStore;
use testcontainers::{ContainerAsync, runners::AsyncRunner};
use testcontainers_modules::redis::Redis;

pub struct TestRedis {
    _container: ContainerAsync<Redis>,
    pub url: String,
    pub store: RedisSessionStore,
}

impl TestRedis {
    pub async fn spawn() -> Self {
        let container = Redis::default()
            .start()
            .await
            .expect("start redis container");
        let host = container.get_host().await.expect("redis host");
        let port = container
            .get_host_port_ipv4(6379)
            .await
            .expect("redis port mapping");
        let url = format!("redis://{host}:{port}/");

        let store = RedisSessionStore::connect(&url)
            .await
            .expect("connect to ephemeral redis");

        Self {
            _container: container,
            url,
            store,
        }
    }
}
