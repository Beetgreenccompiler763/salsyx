//! GitHub webhook endpoint (groundwork for event-driven refreshes).
//!
//! `POST /webhook/github` — receive GitHub webhook deliveries and enqueue
//! repository refreshes. Signatures are verified with the shared webhook
//! secret (`AH_GITHUB__WEBHOOK_SECRET`) using `X-Hub-Signature-256`
//! (HMAC-SHA256). When no secret is configured the endpoint still accepts
//! deliveries but is intended for development only.
//!
//! The crawler already polls `crawl_jobs`, so the webhook simply enqueues the
//! same durable work the API's `POST /archive` and `POST /refresh` do — no
//! new worker plumbing required.

use axum::{
    body::Bytes,
    extract::State,
    http::HeaderMap,
    Json,
};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::error::AppError;
use crate::state::AppState;

const SIGNATURE_HEADER: &str = "x-hub-signature-256";
const EVENT_HEADER: &str = "x-github-event";

#[derive(Debug, Serialize)]
pub struct WebhookResponse {
    pub received: bool,
    pub event: String,
    /// Repositories that were scheduled for a refresh.
    pub scheduled: Vec<String>,
}

/// Minimal GitHub webhook payload — only the fields we need.
#[derive(Debug, Deserialize)]
struct WebhookPayload {
    #[serde(default)]
    repository: Option<WebhookRepository>,
}

#[derive(Debug, Deserialize)]
struct WebhookRepository {
    full_name: String,
}

fn verify_signature(secret: &str, body: &[u8], signature: Option<&str>) -> bool {
    let Some(signature) = signature else {
        return false;
    };
    let Some(hex_sig) = signature.strip_prefix("sha256=") else {
        return false;
    };

    let mut mac = match Hmac::<Sha256>::new_from_slice(secret.as_bytes()) {
        Ok(mac) => mac,
        Err(_) => return false,
    };
    mac.update(body);

    use hex::ToHex;
    let expected = mac.finalize().into_bytes().encode_hex::<String>();
    // Constant-time compare to avoid leaking the secret via timing.
    constant_time_eq(expected.as_bytes(), hex_sig.as_bytes())
}

/// Constant-time byte equality (no short-circuit on mismatch).
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// `POST /webhook/github`
pub async fn webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<WebhookResponse>, AppError> {
    let event = headers
        .get(EVENT_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    let signature = headers
        .get(SIGNATURE_HEADER)
        .and_then(|v| v.to_str().ok());

    // Enforce the signature whenever a secret is configured.
    if let Some(secret) = state
        .config
        .github
        .webhook_secret
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        if !verify_signature(secret, &body, signature) {
            tracing::warn!(event, "rejected webhook with invalid signature");
            return Err(AppError::Validation("invalid webhook signature".into()));
        }
    }

    let payload = serde_json::from_slice::<WebhookPayload>(&body).ok();

    let mut scheduled = Vec::new();
    if let Some(payload) = payload {
        let full_name = payload.repository.map(|r| r.full_name);
        if let Some(full_name) = full_name {
            // Schedule a refresh: resolve live, which refreshes metadata and
            // (via the archive job the crawler already runs on 404s) guards
            // against imminent deletion. Enqueue idempotently.
            if let Ok(result) = crate::service::resolve_repository(&state, &full_name, true).await {
                use crate::service::ResolveOutcome;
                if let ResolveOutcome::Live { repository, .. } = &result.outcome {
                    let _ =
                        crate::db::enqueue_crawl_job(&state.pool, repository.id, None, "archive")
                            .await;
                    scheduled.push(full_name);
                }
            }
        }
    }

    Ok(Json(WebhookResponse {
        received: true,
        event,
        scheduled,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex::ToHex;

    fn sign(secret: &str, body: &[u8]) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        format!("sha256={}", mac.finalize().into_bytes().encode_hex::<String>())
    }

    #[test]
    fn signature_verifies_with_correct_hmac() {
        let secret = "s3cr3t";
        let body = br#"{"repository":{"full_name":"octocat/Hello-World"}}"#;
        let sig = sign(secret, body);
        assert!(verify_signature(secret, body, Some(&sig)));
    }

    #[test]
    fn signature_rejects_missing_or_malformed_header() {
        let secret = "s3cr3t";
        let body = b"{}";
        assert!(!verify_signature(secret, body, None));
        assert!(!verify_signature(secret, body, Some("sha1=abc")));
        assert!(!verify_signature(secret, body, Some("garbage")));
    }

    #[test]
    fn signature_rejects_wrong_key_or_tampered_body() {
        let secret = "s3cr3t";
        let body = b"payload";
        let sig = sign(secret, body);
        assert!(!verify_signature("other", body, Some(&sig)));
        assert!(!verify_signature(secret, b"tampered", Some(&sig)));
    }

    #[test]
    fn constant_time_eq_has_no_len_short_circuit_and_matches() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
    }
}
