//! Health endpoint — liveness and dependency checks.

use axum::Json;
use serde::Serialize;

use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub database: &'static str,
    pub uptime_secs: u64,
}

static STARTED: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

/// `GET /api/v1/health`
pub async fn health(state: axum::extract::State<AppState>) -> Json<HealthResponse> {
    let started = STARTED.get_or_init(std::time::Instant::now);

    // Non-fatal DB ping; the endpoint still returns 200 so load balancers
    // treat this as liveness, and the field exposes readiness.
    let db_status = match sqlx::query("SELECT 1").execute(&state.pool).await {
        Ok(_) => "ok",
        Err(_) => "unreachable",
    };

    if db_status == "unreachable" {
        tracing::warn!("health check: database unreachable");
    }

    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        database: db_status,
        uptime_secs: started.elapsed().as_secs(),
    })
}
