//! HTTP routes for the Salsyx API.
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

pub mod admin;
pub mod archive;
pub mod health;
pub mod owner;
pub mod readme;
pub mod repo;
pub mod search;
pub mod stats;
pub mod webhook;

/// Build the API router.
pub fn build(_config: &Config, state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(health::health))
        .route("/search", get(search::search))
        .route("/repo/{owner}/{repo}", get(repo::resolve))
        .route("/repo/{owner}/{repo}/readme", get(readme::get_readme))
        .route("/repo/{owner}/{repo}/archives", get(repo::history))
        .route("/owner/{login}", get(owner::get_owner))
        .route("/archive/{id}", get(archive::get_archive))
        .route("/archive/{id}/tree", get(archive::tree))
        .route("/archive/{id}/blob", get(archive::blob))
        .route("/download/{id}", get(archive::download))
        .route("/stats", get(stats::stats))
        .route("/stats/top", get(stats::top))
        .route("/admin/overview", get(admin::overview))
        .route("/archive", post(archive::create_archive))
        .route("/refresh", post(repo::refresh))
        .with_state(state.clone());

    // GitHub webhook lives outside `/api/v1` so repo admins can point the
    // GitHub integration straight at the base URL.
    let webhook = Router::new()
        .route("/webhook/github", post(webhook::webhook))
        .with_state(state);

    // Serve the OpenAPI document at a stable path.
    Router::new()
        .nest("/api/v1", api)
        .merge(webhook)
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
