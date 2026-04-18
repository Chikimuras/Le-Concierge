//! Binary entrypoint.
//!
//! Responsibilities, in order:
//!
//! 1. Load configuration ([`api::config::Config`]).
//! 2. Initialize tracing so every subsequent log line is structured.
//! 3. Build the application state (lazy DB pool, no network IO yet).
//! 4. Bind the TCP listener and serve.
//! 5. Gracefully shut down on `SIGINT` / `SIGTERM`.
//!
//! `anyhow` is used here — and only here — per `CLAUDE.md` §2.1.

use std::net::SocketAddr;

use anyhow::Context;
use tokio::signal;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = api::Config::load().context("loading configuration")?;

    api::telemetry::init(
        config.telemetry.format,
        &config.telemetry.filter,
        &config.telemetry.service_name,
    )
    .context("initializing telemetry")?;

    tracing::info!(
        config = %config.log_summary(),
        "starting api",
    );

    let bind = config.http.bind;
    let state = api::AppState::new(config)
        .await
        .context("initializing application state")?;
    let app = api::build_app(state);

    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .with_context(|| format!("binding {bind}"))?;
    let local_addr = listener.local_addr().context("reading listener addr")?;
    tracing::info!(%local_addr, "listening");

    // `into_make_service_with_connect_info` makes `ConnectInfo<SocketAddr>`
    // available as an extractor — needed by the auth handlers to audit
    // the caller's IP address.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .context("serving")?;

    tracing::info!("bye");
    Ok(())
}

/// Resolves when a `SIGINT` (Ctrl+C) or `SIGTERM` is received.
///
/// `SIGTERM` is the signal sent by Docker / Kubernetes on `docker stop` /
/// pod eviction, so we must listen for it explicitly.
///
/// Uses `.expect(...)` because failure to install a signal handler at
/// boot is unrecoverable — per CLAUDE.md §7.1 `expect` is permitted in
/// bootstrap.
#[allow(clippy::expect_used)]
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => tracing::info!("SIGINT received, shutting down"),
        () = terminate => tracing::info!("SIGTERM received, shutting down"),
    }
}
