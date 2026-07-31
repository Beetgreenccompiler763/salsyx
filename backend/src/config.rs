//! Application configuration.
//!
//! Configuration is loaded from, in order of precedence:
//!
//! 1. Environment variables (prefix `AH_`)
//! 2. `.env` file in the working directory
//! 3. `config/default.toml` if present
//!
//! The pattern is: sensible local-development defaults baked in, everything
//! overridable via env vars, and secrets always injected from the
//! environment (never committed).

use std::net::SocketAddr;

use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use serde::Deserialize;

/// Top-level configuration tree.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// HTTP server binding. Defaults to `0.0.0.0:8080` (Fly/Railway friendly).
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub github: GithubConfig,
    pub storage: StorageConfig,
    pub queue: QueueConfig,
    pub app: AppConfig,
    pub providers: ProvidersConfig,
}

/// External archive providers.
#[derive(Debug, Clone, Deserialize)]
pub struct ProvidersConfig {
    /// Provider slugs to skip in the resolution chain, e.g. `["archive_org"]`.
    /// Defaults to empty (all providers enabled).
    #[serde(default)]
    pub disabled: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    /// CORS allowed origin for the frontend.
    pub allowed_origin: String,
    /// Path to the static OpenAPI document served at `/openapi.json`.
    pub openapi_path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    /// Maximum pool size.
    pub max_connections: u32,
    /// Seconds before a connection acquisition times out.
    pub acquire_timeout_secs: u64,
    /// Run pending migrations on startup.
    pub auto_migrate: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GithubConfig {
    /// GitHub API base URL. Defaults to `https://api.github.com`.
    pub base_url: String,
    /// Optional PAT. Increases the rate limit from 60 to 5000 req/h.
    pub token: Option<String>,
    /// `User-Agent` sent to the GitHub API (required by GitHub's ToS).
    pub user_agent: String,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
    /// Shared secret for verifying `X-Hub-Signature-256` on the GitHub
    /// webhook endpoint. Empty/absent disables signature enforcement.
    pub webhook_secret: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    /// Storage provider: `local` (filesystem, for dev) or `r2`.
    pub provider: String,
    /// Root directory when using the local provider.
    pub local_root: String,
    // Cloudflare R2 (S3-compatible) settings.
    pub r2_endpoint: Option<String>,
    pub r2_bucket: Option<String>,
    pub r2_account_id: Option<String>,
    pub r2_access_key_id: Option<String>,
    pub r2_secret_access_key: Option<String>,
    /// Optional public URL prefix used to build download links.
    pub r2_public_base_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QueueConfig {
    /// Queue capacity for the in-memory worker channel.
    pub capacity: usize,
    /// How many events the crawler worker pool should process concurrently.
    pub worker_concurrency: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub env: String,
    /// Allow anonymous `POST /archive` (rate limited) — dev convenience.
    pub allow_public_archive_requests: bool,
    /// Seconds after which a repository refresh is considered stale and
    /// should be re-checked.
    pub refresh_ttl_secs: i64,
}

impl Config {
    /// Load configuration from the environment and optional `.env` file.
    pub fn load() -> anyhow::Result<Self> {
        let _ = dotenvy::dotenv();

        // Locate the default TOML regardless of the current working directory
        // (workspace root, `backend/`, or a container where it's baked in).
        // The TOML provides low-priority defaults; env vars (AH_*) win.
        let mut figment = Figment::new();

        for candidate in [
            "config/default.toml",
            "backend/config/default.toml",
            "/app/config/default.toml",
        ] {
            if std::path::Path::new(candidate).exists() {
                figment = figment.merge(Toml::file(candidate));
                break;
            }
        }

        figment = figment.merge(Env::prefixed("AH_").split("__"));

        let mut config: Config = figment.extract()?;

        // Provide a sensible default server host based on environment.
        if config.server.host.is_empty() {
            config.server.host = if config.app.env == "production" {
                "0.0.0.0".to_string()
            } else {
                "127.0.0.1".to_string()
            };
        }

        Ok(config)
    }

    /// Resolved socket address to bind the HTTP server to.
    pub fn socket_addr(&self) -> SocketAddr {
        format!("{}:{}", self.server.host, self.server.port)
            .parse()
            .unwrap_or_else(|_| panic!("invalid socket address in config"))
    }
}
