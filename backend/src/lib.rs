//! Salsyx API server library.
//!
//! Exposes the application as a library so integration tests can spin up
//! routers without spawning a process. `main.rs` is a thin bootstrap layer.

pub mod config;
pub mod db;
pub mod error;
pub mod git;
pub mod github;
pub mod providers;
pub mod queue;
pub mod routes;
pub mod service;
pub mod state;
pub mod storage;
pub mod telemetry;

/// Convenience re-export of the shared crate for internal modules.
pub use salsyx_shared as shared;
