//! Application configuration.
//!
//! Config is composed from, in order (later overrides earlier):
//!
//! 1. `config/default.toml` — baseline values checked into the repo.
//! 2. A file pointed to by the `APP_CONFIG_FILE` env var, if set.
//! 3. Environment variables prefixed with `APP_`, using `__` as the nesting
//!    separator (e.g. `APP_HTTP__BIND=0.0.0.0:3000`).
//!
//! Secrets (database URL, pepper, Stripe keys, …) **must** come from env
//! variables or Docker secrets, never from the TOML file. See
//! `CLAUDE.md` §3.3 and ADR 0002.

use std::{net::SocketAddr, path::PathBuf};

use figment::{
    Figment,
    providers::{Env, Format, Toml},
};
use serde::Deserialize;

/// Root configuration tree. Every field is required; defaults are supplied by
/// `config/default.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub http: HttpConfig,
    pub database: DatabaseConfig,
    pub telemetry: TelemetryConfig,
    pub cors: CorsConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpConfig {
    /// Address the HTTP server binds to. Prod behind a reverse proxy usually
    /// binds to `0.0.0.0:3000`; local dev should stay on loopback.
    pub bind: SocketAddr,
    /// Per-request timeout in seconds. Applies to the whole request lifecycle
    /// via `tower_http::timeout::TimeoutLayer`.
    pub request_timeout_secs: u64,
    /// Public base URL the API is reachable at. Used for OpenAPI server URL
    /// and for constructing absolute links in responses.
    pub public_base_url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseConfig {
    /// PostgreSQL connection string. Treated as a secret: never logged.
    ///
    /// Typical value: `postgres://user:pass@host:5432/le_concierge`.
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    /// Upper bound on how long a statement can run before SQLx cancels it.
    pub statement_timeout_secs: u64,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// Structured JSON lines. Required in production per CLAUDE.md §5.
    Json,
    /// Human-friendly multi-line output. Dev only.
    Pretty,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TelemetryConfig {
    pub format: LogFormat,
    /// `EnvFilter` directive string (e.g. `api=debug,tower_http=info,warn`).
    pub filter: String,
    /// Service name advertised in log records and future OTLP exports.
    pub service_name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorsConfig {
    /// Explicit allow-list of origins. An empty list disables CORS, matching
    /// OWASP ASVS 14.5.3 (no wildcard with credentials).
    pub allowed_origins: Vec<String>,
}

impl Config {
    /// Load configuration from the layered sources documented at module level.
    ///
    /// # Errors
    ///
    /// Returns an error if the TOML fails to parse, a required field is
    /// missing, or env var deserialization fails.
    pub fn load() -> anyhow::Result<Self> {
        // In dev, `.env` is picked up automatically. In prod, env vars come
        // from Docker secrets / systemd / etc.; the absence of `.env` is fine.
        let _ = dotenvy::dotenv();

        let mut figment = Figment::new().merge(Toml::file("config/default.toml"));

        if let Ok(overlay) = std::env::var("APP_CONFIG_FILE") {
            let overlay: PathBuf = overlay.into();
            figment = figment.merge(Toml::file(overlay));
        }

        figment = figment.merge(Env::prefixed("APP_").split("__"));

        figment.extract::<Self>().map_err(anyhow::Error::from)
    }

    /// Produce a log-safe summary of the configuration. Crucially this
    /// **never** includes the database URL, CORS allow-list, or any other
    /// field that might leak secrets or PII in logs. CLAUDE.md §3.3.
    #[must_use]
    pub fn log_summary(&self) -> serde_json::Value {
        serde_json::json!({
            "http": {
                "bind": self.http.bind.to_string(),
                "request_timeout_secs": self.http.request_timeout_secs,
                "public_base_url": self.http.public_base_url,
            },
            "database": {
                "max_connections": self.database.max_connections,
                "min_connections": self.database.min_connections,
                "statement_timeout_secs": self.database.statement_timeout_secs,
            },
            "telemetry": {
                "format": match self.telemetry.format {
                    LogFormat::Json => "json",
                    LogFormat::Pretty => "pretty",
                },
                "filter": self.telemetry.filter,
                "service_name": self.telemetry.service_name,
            },
            "cors": {
                "allowed_origins_count": self.cors.allowed_origins.len(),
            },
        })
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)] // test assertions
mod tests {
    use super::*;

    #[test]
    fn log_summary_excludes_database_url() {
        let config = Config {
            http: HttpConfig {
                bind: "127.0.0.1:3000".parse().expect("valid addr"),
                request_timeout_secs: 30,
                public_base_url: "http://localhost:3000".into(),
            },
            database: DatabaseConfig {
                url: "postgres://alice:super-secret@db:5432/app".into(),
                max_connections: 10,
                min_connections: 0,
                statement_timeout_secs: 5,
            },
            telemetry: TelemetryConfig {
                format: LogFormat::Pretty,
                filter: "info".into(),
                service_name: "api".into(),
            },
            cors: CorsConfig {
                allowed_origins: vec!["https://example.test".into()],
            },
        };

        let summary = config.log_summary().to_string();
        assert!(
            !summary.contains("super-secret"),
            "database password must never appear in log summaries"
        );
        assert!(
            !summary.contains("example.test"),
            "CORS origins must not be individually logged"
        );
    }
}
