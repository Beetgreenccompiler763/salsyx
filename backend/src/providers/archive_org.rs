//! Archive.org provider.
//!
//! Archive.org is a natural archival partner, but it currently has no stable
//! public API for discovering items by their *source GitHub URL*. The
//! provider is registered in the chain so the wiring is in place and real as
//! soon as a discovery mechanism exists (e.g. saving repos to archive.org
//! items with a `github_url` metadata field).
//!
//! Until then, [`ArchiveOrgProvider`] always answers "no archive" without
//! erroring — matching the behaviour the resolver expects from a provider
//! that does not have the content.

use super::{ArchiveProvider, ExternalArchive, ProviderError};

/// Archive.org provider (currently a discovery placeholder — see module docs).
pub struct ArchiveOrgProvider;

#[async_trait::async_trait]
impl ArchiveProvider for ArchiveOrgProvider {
    fn name(&self) -> &'static str {
        "archive_org"
    }

    async fn lookup(&self, _full_name: &str) -> Result<Option<ExternalArchive>, ProviderError> {
        tracing::debug!("archive.org lookup not yet wired to a discovery API");
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn resolves_without_error() {
        let provider = ArchiveOrgProvider;
        assert!(provider
            .lookup("lmdelm-dev/salsyx")
            .await
            .unwrap()
            .is_none());
    }
}
