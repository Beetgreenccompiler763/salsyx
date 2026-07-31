//! HTTP routes for the ArchiveHub API.
//!
//! Endpoints (all under `/api/v1`):
//!
//! - `GET  /health`          → liveness + dependency status
//! - `GET  /search`          → search repositories (name/owner/desc/language/...)
//! - `GET  /repo/{owner}/{repo}` → resolve a repository against GitHub + archive
//! - `GET  /archive/{id}`    → archive metadata
//! - `GET  /download/{id}`   → stream an archived blob
//! - `GET  /stats`           → platform statistics
//! - `POST /archive`         → enqueue an archive job
//! - `POST /refresh`         → force re-resolve + refresh a repository
//! - `GET  /openapi.json`    → OpenAPI document
//!
//! GraphQL can be added later behind `/graphql` without touching these
//! routes — the services they call are transport-agnostic.

use axum::{
    routing::{get, post},
    Router,
};

use crate::config::Config;
use crate::state::AppState;

pub mod archive;
pub mod health;
pub mod repo;
pub mod search;
pub mod stats;

/// Build the API router.
pub fn build(_config: &Config, state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(health::health))
        .route("/search", get(search::search))
        .route("/repo/{owner}/{repo}", get(repo::resolve))
        .route("/archive/{id}", get(archive::get_archive))
        .route("/download/{id}", get(archive::download))
        .route("/stats", get(stats::stats))
        .route("/stats/top", get(stats::top))
        .route("/archive", post(archive::create_archive))
        .route("/refresh", post(repo::refresh))
        .with_state(state);

    // Serve the OpenAPI document at a stable path.
    Router::new()
        .nest("/api/v1", api)
        .route(
            "/openapi.json",
            get(|| async {
                let doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/openapi.json"));
                axum::Json(serde_json::from_str::<serde_json::Value>(doc).expect("valid openapi"))
            }),
        )
        .route(
            "/openapi",
            get(|| async { axum::response::Redirect::permanent("/openapi.json") }),
        )
}
