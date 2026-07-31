//! Structured logging and tracing setup.
//!
//! Defaults to pretty human-readable output in development and JSON output
//! in production (so Sentry / log ingestion stays clean).
//!
//! # Sentry
//!
//! When the `sentry` feature is enabled and `AH_SENTRY_DSN` is set, error
//! tracking + tracing events are shipped to Sentry automatically.

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Kept alive for the process lifetime so Sentry buffers flush on shutdown.
/// The inner guard is intentionally never read; holding it is the point.
#[cfg(feature = "sentry")]
pub struct SentryGuard(#[allow(dead_code)] sentry::ClientInitGuard);

/// Initialize the global tracing subscriber.
#[cfg(feature = "sentry")]
pub fn init(app_env: &str) -> SentryGuard {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        if app_env == "production" {
            EnvFilter::new("salsyx_api=info,tower_http=info,sqlx=warn,info")
        } else {
            EnvFilter::new("salsyx_api=debug,tower_http=debug,sqlx=debug,debug")
        }
    });

    let sentry_guard = init_sentry(app_env);
    let sentry_layer = sentry_tracing::layer().event_filter(|md| match *md.level() {
        tracing::Level::ERROR => sentry_tracing::EventFilter::Exception,
        tracing::Level::WARN => sentry_tracing::EventFilter::Event,
        _ => sentry_tracing::EventFilter::Ignore,
    });

    if app_env == "production" {
        tracing_subscriber::registry()
            .with(filter)
            .with(sentry_layer)
            .with(
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_target(true)
                    .with_timer(tracing_subscriber::fmt::time()),
            )
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(sentry_layer)
            .with(tracing_subscriber::fmt::layer().pretty().with_target(true))
            .init();
    }

    SentryGuard(sentry_guard)
}

/// Initialize the global tracing subscriber (without Sentry).
#[cfg(not(feature = "sentry"))]
pub fn init(app_env: &str) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        if app_env == "production" {
            EnvFilter::new("salsyx_api=info,tower_http=info,sqlx=warn,info")
        } else {
            EnvFilter::new("salsyx_api=debug,tower_http=debug,sqlx=debug,debug")
        }
    });

    if app_env == "production" {
        tracing_subscriber::registry()
            .with(filter)
            .with(
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_target(true)
                    .with_timer(tracing_subscriber::fmt::time()),
            )
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer().pretty().with_target(true))
            .init();
    }
}

/// Configure and start the Sentry SDK if a DSN is present.
#[cfg(feature = "sentry")]
fn init_sentry(app_env: &str) -> sentry::ClientInitGuard {
    use sentry::ClientOptions;

    let Ok(dsn) = std::env::var("AH_SENTRY_DSN") else {
        tracing::warn!("sentry feature enabled but AH_SENTRY_DSN is not set; skipping");
        return sentry::init(());
    };
    let dsn = dsn.trim();
    if dsn.is_empty() {
        return sentry::init(());
    }

    tracing::info!("initializing sentry error tracking");

    sentry::init((
        dsn.to_string(),
        ClientOptions {
            environment: Some(app_env.to_string().into()),
            release: Some(env!("CARGO_PKG_VERSION").to_string().into()),
            traces_sample_rate: 0.1,
            ..ClientOptions::default()
        },
    ))
}

// Re-export so `main.rs` can hold the guard without importing sentry itself.
#[cfg(feature = "sentry")]
pub use SentryGuard as _SentryGuard;
