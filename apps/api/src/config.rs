//! Application configuration.
//!
//! Config is composed from, in order (later overrides earlier):
//!
//! 1. The baseline `config/default.toml` checked into the repo, **embedded
//!    into the binary at compile time** via `include_str!`. This means the
//!    api boots identically whether it is launched from the workspace root
//!    (`cargo run -p api`), from the crate dir, from `/app/api` inside
//!    the Docker image, or via `systemd` — no CWD assumption.
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
use secrecy::SecretString;
use serde::Deserialize;

/// Baseline configuration baked into the binary. Path is relative to this
/// source file; `include_str!` is evaluated by rustc at compile time.
const DEFAULT_CONFIG_TOML: &str = include_str!("../config/default.toml");

/// Root configuration tree. Every field is required; defaults are supplied by
/// `config/default.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub http: HttpConfig,
    pub database: DatabaseConfig,
    pub telemetry: TelemetryConfig,
    pub cors: CorsConfig,
    pub auth: AuthConfig,
    pub session: SessionConfig,
    pub redis: RedisConfig,
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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthConfig {
    /// Application-wide pepper injected into every Argon2id password hash.
    /// Treated as a secret: never ships in TOML, only via env or Docker
    /// secret (CLAUDE.md §3.1 / ADR 0005). Minimum 32 random bytes
    /// recommended; generate one with `openssl rand -hex 32`.
    ///
    /// `SecretString` redacts itself in `Debug` and zeroes on drop.
    pub pepper: SecretString,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionConfig {
    /// Idle timeout. The session is kept alive while the user keeps
    /// hitting the API; after this many seconds of inactivity, the
    /// Redis key expires and the cookie becomes useless.
    pub idle_ttl_secs: u64,
    /// Absolute cut-off. Regardless of activity, a session older than
    /// this is refused.
    pub absolute_ttl_secs: u64,
    /// Whether the `Secure` cookie attribute is set. Must be `true` in
    /// production; `false` is only safe over a trusted loopback or in
    /// dev.
    pub cookie_secure: bool,
    /// Optional `Domain` attribute. Leaving unset pins the cookie to
    /// the origin host, which is usually what you want for first-party
    /// apps.
    #[serde(default)]
    pub cookie_domain: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedisConfig {
    /// `redis://user:pass@host:port/db` URL. Treated as a secret — never
    /// logged — though it is less critical than the pepper since the
    /// Redis instance is loopback-only in dev and VPC-only in prod.
    pub url: String,
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

        let mut figment = Figment::new().merge(Toml::string(DEFAULT_CONFIG_TOML));

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
            // `auth.pepper` is intentionally omitted — it is a secret
            // (CLAUDE.md §3.3).
            "auth": {
                "pepper_configured": true,
            },
            "session": {
                "idle_ttl_secs": self.session.idle_ttl_secs,
                "absolute_ttl_secs": self.session.absolute_ttl_secs,
                "cookie_secure": self.session.cookie_secure,
                "cookie_domain_set": self.session.cookie_domain.is_some(),
            },
            // `redis.url` may contain the auth password — summary shows
            // only whether it is configured.
            "redis": {
                "configured": !self.redis.url.is_empty(),
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
            auth: AuthConfig {
                pepper: SecretString::from("dangerous-test-pepper-never-use-in-prod"),
            },
            session: SessionConfig {
                idle_ttl_secs: 3600,
                absolute_ttl_secs: 86400,
                cookie_secure: true,
                cookie_domain: None,
            },
            redis: RedisConfig {
                url: "redis://redis.example:6379/".into(),
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
        assert!(
            !summary.contains("dangerous-test-pepper"),
            "auth pepper must never appear in log summaries"
        );
        assert!(
            !summary.contains("redis.example"),
            "redis URL must never appear in log summaries"
        );
    }
}
