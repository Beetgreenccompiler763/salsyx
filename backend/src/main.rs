//! ArchiveHub API server — entry point.
//!
//! Bootstrap order:
//! 1. Load configuration (`AH_*` env vars / `.env` / `config/default.toml`)
//! 2. Initialize structured logging
//! 3. Connect to Postgres (apply migrations if configured)
//! 4. Build the storage backend and GitHub client
//! 5. Start the HTTP server with CORS + trace middleware

use archivehub_api::config::Config;
use archivehub_api::state::AppState;
use archivehub_api::telemetry;
use axum::http::HeaderName;
use tower::ServiceBuilder;
use tower_http::{
    catch_panic::CatchPanicLayer,
    compression::CompressionLayer,
    cors::CorsLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::load()?;
    // With the `sentry` feature, keep the guard alive for the process
    // lifetime so buffered events flush on shutdown.
    #[cfg(feature = "sentry")]
    let _sentry = telemetry::init(&config.app.env);
    #[cfg(not(feature = "sentry"))]
    telemetry::init(&config.app.env);

    tracing::info!(version = env!("CARGO_PKG_VERSION"), env = %config.app.env, "starting archivehub-api");

    let state = AppState::from_config(config.clone()).await?;

    // CORS: allow the configured frontend origin (any in dev).
    let cors = if config.server.allowed_origin == "*" {
        CorsLayer::permissive()
    } else {
        CorsLayer::new().allow_origin(
            config
                .server
                .allowed_origin
                .parse::<axum::http::HeaderValue>()
                .expect("invalid allowed_origin in config"),
        )
    };

    let app = archivehub_api::routes::build(&config, state).layer(
        ServiceBuilder::new()
            .layer(SetRequestIdLayer::new(
                HeaderName::from_static("x-request-id"),
                MakeRequestUuid,
            ))
            .layer(PropagateRequestIdLayer::x_request_id())
            .layer(TraceLayer::new_for_http())
            .layer(CompressionLayer::new())
            .layer(CatchPanicLayer::new())
            .layer(TimeoutLayer::with_status_code(
                axum::http::StatusCode::GATEWAY_TIMEOUT,
                std::time::Duration::from_secs(60),
            ))
            .layer(cors),
    );

    let addr = config.socket_addr();
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

/// Graceful shutdown on SIGINT / SIGTERM (Docker/K8s/Fly friendly).
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("shutdown signal received");
}
