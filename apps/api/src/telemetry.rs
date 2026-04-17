//! Tracing / logging initialization.
//!
//! Production logs are structured JSON with one record per line, flattened
//! event fields, and the full span list — matching the contract in
//! `CLAUDE.md` §5 and enabling ingestion by Loki/Tempo downstream.
//! Development logs use a pretty, multi-line format that is friendlier to
//! skim.
//!
//! Only `init` is public. It must be called exactly once, before any
//! `tracing::` macros elsewhere in the program emit useful output.

use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::LogFormat;

/// Install the global tracing subscriber.
///
/// # Errors
///
/// Returns an error if `filter` is not a valid `EnvFilter` directive, or if
/// another subscriber has already been installed (tests that share a process
/// must gate initialization with `std::sync::Once`).
pub fn init(format: LogFormat, filter: &str, service_name: &str) -> anyhow::Result<()> {
    let env_filter = EnvFilter::try_new(filter)
        .map_err(|e| anyhow::anyhow!("invalid tracing filter `{filter}`: {e}"))?;

    // Only one of the two fmt layers is ever Some. The Layer impl on
    // Option<L> makes the inactive one a no-op at zero runtime cost.
    let (json_layer, pretty_layer) = match format {
        LogFormat::Json => (
            Some(
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_target(true)
                    .with_current_span(false)
                    .with_span_list(true)
                    .flatten_event(true),
            ),
            None,
        ),
        LogFormat::Pretty => (
            None,
            Some(
                tracing_subscriber::fmt::layer()
                    .pretty()
                    .with_target(true)
                    .with_line_number(false),
            ),
        ),
    };

    tracing_subscriber::registry()
        .with(env_filter)
        .with(json_layer)
        .with(pretty_layer)
        .try_init()
        .map_err(|e| anyhow::anyhow!("tracing init failed: {e}"))?;

    tracing::info!(
        service.name = %service_name,
        format = ?format,
        "telemetry initialized"
    );

    Ok(())
}

// No unit tests in this module: `init` installs a process-global subscriber,
// which makes it hard to exercise in isolation without a custom
// `Registry` set-up. Behaviour is covered by the integration tests that
// spawn the real app (see `tests/health.rs`).
