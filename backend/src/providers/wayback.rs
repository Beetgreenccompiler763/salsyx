//! Wayback Machine provider (archive.org).
//!
//! Uses the Wayback availability API to find the closest captured snapshot of
//! a GitHub repository page, and links users to it. Because the Wayback
//! Machine snapshots web pages (not git repositories), this is best-effort:
//! it may capture a README page or nothing at all.

use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::Deserialize;

use super::{ArchiveProvider, ExternalArchive, ProviderError};

/// Wayback availability API endpoint.
const AVAILABILITY_API: &str = "https://archive.org/wayback/available";

#[derive(Debug, Deserialize)]
struct AvailabilityResponse {
    archived_snapshots: Snapshots,
}

#[derive(Debug, Deserialize)]
struct Snapshots {
    #[serde(rename = "closest")]
    closest: Option<Snapshot>,
}

#[derive(Debug, Deserialize)]
struct Snapshot {
    status: String,
    available: bool,
    url: String,
    timestamp: String,
}

/// Wayback Machine provider.
pub struct WaybackProvider {
    client: Client,
    api: String,
}

impl WaybackProvider {
    /// `api` defaults to the public Wayback availability endpoint.
    pub fn new(client: Client) -> Self {
        Self {
            client,
            api: AVAILABILITY_API.to_string(),
        }
    }

    pub fn with_api(client: Client, api: impl Into<String>) -> Self {
        Self {
            client,
            api: api.into(),
        }
    }
}

#[async_trait::async_trait]
impl ArchiveProvider for WaybackProvider {
    fn name(&self) -> &'static str {
        "wayback"
    }

    async fn lookup(&self, full_name: &str) -> Result<Option<ExternalArchive>, ProviderError> {
        let github_url = format!("github.com/{full_name}");
        let resp = self
            .client
            .get(&self.api)
            .query(&[("url", github_url.as_str())])
            .send()
            .await
            .map_err(|e| ProviderError::Upstream(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ProviderError::Upstream(format!(
                "wayback availability returned {}",
                resp.status()
            )));
        }

        let payload: AvailabilityResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::Upstream(e.to_string()))?;

        let Some(closest) = payload.archived_snapshots.closest else {
            return Ok(None);
        };
        if !closest.available || closest.status != "200" {
            return Ok(None);
        }

        let captured_at = parse_timestamp(&closest.timestamp);

        Ok(Some(ExternalArchive {
            provider: self.name(),
            browse_url: closest.url,
            download_url: None,
            captured_at,
            note: "A snapshot of this repository's page was captured by the Internet Archive's Wayback Machine.".to_string(),
        }))
    }
}

/// Parse `YYYYMMDDHHmmss` (or prefix) into a UTC timestamp.
fn parse_timestamp(ts: &str) -> Option<DateTime<Utc>> {
    use chrono::NaiveDate;
    if ts.len() < 8 {
        return None;
    }
    let (year, month, day) = (&ts[0..4], &ts[4..6], &ts[6..8]);
    let date = NaiveDate::from_ymd_opt(year.parse().ok()?, month.parse().ok()?, day.parse().ok()?)?;
    let (hour, minute, second) = (
        ts.get(8..10).and_then(|s| s.parse().ok()).unwrap_or(0),
        ts.get(10..12).and_then(|s| s.parse().ok()).unwrap_or(0),
        ts.get(12..14).and_then(|s| s.parse().ok()).unwrap_or(0),
    );
    let datetime = date
        .and_hms_opt(hour, minute, second)
        .unwrap_or_else(|| date.and_hms_opt(0, 0, 0).expect("valid hms"));
    Some(chrono::TimeZone::from_utc_datetime(&Utc, &datetime))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn returns_archive_when_snapshot_exists() {
        let (addr, _guard) = crate::providers::software_heritage::mock_server(
            200,
            r#"{"archived_snapshots":{"closest":{"status":"200","available":true,"url":"http://web.archive.org/web/20250315120000/https://github.com/lmdelm-dev/launchbis","timestamp":"20250315120000"}}}"#,
        )
        .await;
        let provider = WaybackProvider::with_api(Client::new(), format!("http://{addr}"));
        let hit = provider.lookup("lmdelm-dev/launchbis").await.unwrap();
        let archive = hit.expect("snapshot exists");
        assert_eq!(archive.provider, "wayback");
        assert!(archive.browse_url.contains("web.archive.org"));
        assert!(archive.captured_at.is_some());
    }

    #[tokio::test]
    async fn returns_none_without_snapshots() {
        let (addr, _guard) = crate::providers::software_heritage::mock_server(
            200,
            r#"{"archived_snapshots":{"closest":null}}"#,
        )
        .await;
        let provider = WaybackProvider::with_api(Client::new(), format!("http://{addr}"));
        assert!(provider.lookup("ghost/repo").await.unwrap().is_none());
    }

    #[test]
    fn parses_timestamps() {
        assert!(parse_timestamp("20250315120000").is_some());
        assert!(parse_timestamp("20250315").is_some());
        assert!(parse_timestamp("bad").is_none());
    }
}
