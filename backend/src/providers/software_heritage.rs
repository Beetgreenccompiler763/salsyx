//! Software Heritage provider.
//!
//! Software Heritage is a non-profit, long-term archive of all publicly
//! available source code. We ask its REST API whether the GitHub origin has
//! been saved; if so, we point users at its browse UI.

use reqwest::Client;
use serde::Deserialize;

use super::{ArchiveProvider, ExternalArchive, ProviderError};

/// Default public Software Heritage API root.
const DEFAULT_API_ROOT: &str = "https://archive.softwareheritage.org/api/1";

/// A saved origin record from `GET /origin/github/{owner}/{repo}/get/`.
#[derive(Debug, Deserialize)]
struct SwhOrigin {
    #[serde(rename = "type")]
    kind: String,
    url: String,
}

/// Software Heritage archive provider.
pub struct SoftwareHeritageProvider {
    client: Client,
    api_root: String,
}

impl SoftwareHeritageProvider {
    /// `api_root` defaults to the public Software Heritage API.
    pub fn new(client: Client) -> Self {
        Self {
            client,
            api_root: DEFAULT_API_ROOT.to_string(),
        }
    }

    pub fn with_api_root(client: Client, api_root: impl Into<String>) -> Self {
        Self {
            client,
            api_root: api_root.into(),
        }
    }

    /// Browse URL for a GitHub origin saved in Software Heritage.
    fn browse_url(full_name: &str) -> String {
        format!(
            "https://archive.softwareheritage.org/browse/origin/https://github.com/{full_name}/"
        )
    }
}

#[async_trait::async_trait]
impl ArchiveProvider for SoftwareHeritageProvider {
    fn name(&self) -> &'static str {
        "software_heritage"
    }

    async fn lookup(&self, full_name: &str) -> Result<Option<ExternalArchive>, ProviderError> {
        let url = format!("{}/origin/github/{}/get/", self.api_root, full_name);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| ProviderError::Upstream(e.to_string()))?;

        match resp.status() {
            reqwest::StatusCode::OK => {
                let origin: SwhOrigin = resp
                    .json()
                    .await
                    .map_err(|e| ProviderError::Upstream(e.to_string()))?;
                Ok(Some(ExternalArchive {
                    provider: self.name(),
                    browse_url: Self::browse_url(full_name),
                    download_url: Some(origin.url),
                    captured_at: None,
                    note: format!(
                        "Saved in Software Heritage ({}) — long-term archival mirror.",
                        origin.kind
                    ),
                }))
            }
            reqwest::StatusCode::NOT_FOUND => Ok(None),
            status => Err(ProviderError::Upstream(format!(
                "software heritage returned {status}"
            ))),
        }
    }
}

/// Placeholder parse helper kept near the type for future date extraction.
/// Spin up a tiny in-process HTTP server returning `status`/`body`.
#[cfg(test)]
pub(super) async fn mock_server(
    status: u16,
    body: &'static str,
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf).await;
                let resp = format!(
                    "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n{body}",
                    len = body.len()
                );
                let _ = stream.write_all(resp.as_bytes()).await;
            });
        }
    });

    (addr, handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn returns_archive_when_origin_saved() {
        let (addr, _guard) = mock_server(
            200,
            r#"{"type":"git","url":"https://github.com/lmdelm-dev/launchbis"}"#,
        )
        .await;
        let client = Client::new();
        let provider = SoftwareHeritageProvider::with_api_root(client, format!("http://{addr}"));
        let hit = provider.lookup("lmdelm-dev/launchbis").await.unwrap();
        let archive = hit.expect("origin is saved");
        assert_eq!(archive.provider, "software_heritage");
        assert!(archive.browse_url.contains("launchbis"));
        assert_eq!(
            archive.download_url.as_deref(),
            Some("https://github.com/lmdelm-dev/launchbis")
        );
    }

    #[tokio::test]
    async fn returns_none_on_404() {
        let (addr, _guard) = mock_server(404, "{}").await;
        let client = Client::new();
        let provider = SoftwareHeritageProvider::with_api_root(client, format!("http://{addr}"));
        assert!(provider.lookup("ghost/repo").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn surfaces_upstream_errors() {
        let (addr, _guard) = mock_server(500, "boom").await;
        let client = Client::new();
        let provider = SoftwareHeritageProvider::with_api_root(client, format!("http://{addr}"));
        assert!(provider.lookup("x/y").await.is_err());
    }
}
