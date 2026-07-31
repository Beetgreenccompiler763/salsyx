//! Salsyx crawler.
//!
//! A standalone background worker that runs the archive pipeline:
//!
//! ```text
//! ┌──────────────┐   poll crawl_jobs    ┌─────────────────┐
//! │  API server   │ ──────────────────► │  crawler worker  │
//! │  (enqueues)   │                     │   (this crate)   │
//! └──────────────┘                      └───────┬─────────┘
//!                                               │
//!                              git clone ────────┤
//!                              compress ────────┤
//!                              checksum ────────┤
//!                              upload ──────────► storage (R2/local)
//!                              update ──────────► database
//!
//! The crawler is intentionally *independent* from the API server: it polls
//! the shared `crawl_jobs` table, so it can run on a different host / fly
//! machine and scale horizontally (multiple workers, one job each).
//!
//! # Job semantics
//! - `archive`: clone the repo, produce a git bundle, store it, verify.
//! - `refresh`: re-resolve the repo against GitHub (metadata freshness).
//! - `verify`:  re-check the stored checksum of an existing archive.
//!
//! Deduplication: a job row exists per repo; `attempts`/`max_attempts` bound
//! retries and a job is never reprocessed while it is `running`.

pub mod jobs;
pub mod pipeline;

/// Number of worker tasks to spawn when `AH_CRAWLER_CONCURRENCY` is unset.
pub const DEFAULT_CONCURRENCY: usize = 4;
