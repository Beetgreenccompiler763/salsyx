//! External archive providers.
//!
//! When a repository no longer exists on GitHub, Salsyx asks a chain of
//! archive providers for a preserved copy:
//!
//! ```text
//! GitHub (primary)
//!   └─ 404 → Software Heritage → Archive.org → Wayback Machine → AAHL local → not found
//! ```
//!
//! Every provider implements the [`ArchiveProvider`] trait, so adding a new
//! source is a single file: implement the trait and register it in
//! [`build_providers`]. Providers are stateless and never talk to the
//! frontend; they only answer "do you have `owner/repo` and where?".

use chrono::{DateTime, Utc};
use reqwest::Client;

pub mod archive_org;
pub mod software_heritage;
pub mod wayback;

pub use archive_org::ArchiveOrgProvider;
pub use software_heritage::SoftwareHeritageProvider;
pub use wayback::WaybackProvider;

/// An archive of `owner/repo` hosted by an external provider.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ExternalArchive {
    /// Provider slug, e.g. `software_heritage`.
    pub provider: &'static str,
    /// Where a human can browse the archived copy.
    pub browse_url: String,
    /// Direct download when the provider exposes one.
    pub download_url: Option<String>,
    /// When the snapshot was captured, if the provider reports it.
    pub captured_at: Option<DateTime<Utc>>,
    /// Human-readable note for the UI.
    pub note: String,
}

/// Errors produced by providers. A failed provider must not abort the chain —
/// [`resolve_external`] logs and moves on.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("upstream error: {0}")]
    Upstream(String),
    #[error("provider misconfigured: {0}")]
    Config(String),
}

/// The contract every archive provider implements.
#[async_trait::async_trait]
pub trait ArchiveProvider: Send + Sync {
    /// Stable provider slug, e.g. `wayback`.
    fn name(&self) -> &'static str;

    /// Return `Some(archive)` when this provider holds a copy of `full_name`
    /// (`owner/repo`), `None` when it doesn't. Errors are treated as a
    /// "skip me" signal by the chain resolver.
    async fn lookup(&self, full_name: &str) -> Result<Option<ExternalArchive>, ProviderError>;
}

/// Walk providers in order, returning the first archive found.
pub async fn resolve_external(
    providers: &[Box<dyn ArchiveProvider>],
    full_name: &str,
) -> Option<ExternalArchive> {
    for provider in providers {
        match provider.lookup(full_name).await {
            Ok(Some(archive)) => {
                tracing::info!(
                    provider = provider.name(),
                    full_name,
                    browse = %archive.browse_url,
                    "external archive found"
                );
                return Some(archive);
            }
            Ok(None) => {
                tracing::debug!(
                    provider = provider.name(),
                    full_name,
                    "no archive at provider"
                );
            }
            Err(e) => {
                tracing::warn!(provider = provider.name(), full_name, error = %e, "provider lookup failed");
            }
        }
    }
    None
}

/// Build the provider chain. `disabled` is a list of provider slugs to skip
/// (e.g. `AH_PROVIDERS__DISABLED="archive_org"`).
pub fn build_providers(disabled: &[String]) -> Vec<Box<dyn ArchiveProvider>> {
    let client = Client::new();
    let mut providers: Vec<Box<dyn ArchiveProvider>> = Vec::new();

    for (slug, provider) in [
        (
            "software_heritage",
            Box::new(SoftwareHeritageProvider::new(client.clone())) as Box<dyn ArchiveProvider>,
        ),
        (
            "archive_org",
            Box::new(ArchiveOrgProvider) as Box<dyn ArchiveProvider>,
        ),
        (
            "wayback",
            Box::new(WaybackProvider::new(client.clone())) as Box<dyn ArchiveProvider>,
        ),
    ] {
        if disabled.iter().any(|d| d == slug) {
            tracing::debug!(slug, "provider disabled by config");
            continue;
        }
        providers.push(provider);
    }
    providers
}
