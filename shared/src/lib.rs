//! Salsyx shared crate.
//!
//! This crate contains the canonical domain types, enums, and constants
//! shared between the API server, the crawler workers, and the storage
//! layer. Keeping them in one place guarantees that every component agrees
//! on the meaning of an archive status, a checksum, or a repository state.
//!
//! # Design notes
//!
//! - Types are intentionally database-agnostic (no sqlx types here). The
//!   persistence layer maps to and from these domain types.
//! - All status enums are serialized as snake_case strings for a stable
//!   public API contract. Never rename a variant after it ships.
//! - Adding a new enum variant is backwards compatible; removing or
//!   renaming one is a breaking change to the public contract.

pub mod archive;
pub mod error;
pub mod events;
pub mod repository;
pub mod search;

/// The current schema version of the public API contract.
pub const API_VERSION: &str = "v1";

/// Name of the canonical header clients must send to request the API version.
pub const API_VERSION_HEADER: &str = "x-salsyx-version";
