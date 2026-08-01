//! HTTP middleware.
//!
//! Currently provides `cache_headers`: a lightweight cache-control header
//! middleware for GET responses, plus an opt-in in-memory cache for stable
//! read endpoints (search/stats/health).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderValue, Request};
use axum::middleware::Next;
use axum::response::Response;

use crate::state::AppState;

/// TTL for cached read responses.
const CACHE_TTL: Duration = Duration::from_secs(30);
/// Headers that indicate the response must not be cached.
const CACHE_BYPASS_HEADER: &str = "x-salsyx-no-cache";

/// Simple in-memory response cache shared via `AppState`.
#[derive(Clone, Default)]
pub struct ResponseCache {
    inner: Arc<Mutex<HashMap<String, CachedEntry>>>,
}

struct CachedEntry {
    body: Vec<u8>,
    content_type: HeaderValue,
    cached_at: Instant,
}

impl ResponseCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: &str) -> Option<Response> {
        let entries = self.inner.lock().ok()?;
        let entry = entries.get(key)?;
        if entry.cached_at.elapsed() > CACHE_TTL {
            return None;
        }
        let response = Response::builder()
            .status(200)
            .header(header::CONTENT_TYPE, entry.content_type.clone())
            .header(header::CACHE_CONTROL, "public, max-age=30")
            .header("x-salsyx-cache", "HIT")
            .body(Body::from(entry.body.clone()))
            .ok()?;
        Some(response)
    }

    pub fn store(&self, key: String, content_type: HeaderValue, body: Vec<u8>) {
        if let Ok(mut entries) = self.inner.lock() {
            if entries.len() > 256 {
                entries.retain(|_, e| e.cached_at.elapsed() < CACHE_TTL);
            }
            entries.insert(
                key,
                CachedEntry {
                    body,
                    content_type,
                    cached_at: Instant::now(),
                },
            );
        }
    }
}

/// Cache-control + in-memory caching middleware for stable GET endpoints.
///
/// Applies a short `Cache-Control` header to every response and caches
/// responses for the whitelisted read routes (`/search`, `/stats`,
/// `/stats/top`, `/health`) for 30s. Requests with a `Cache-Control:
/// no-cache` header or an `x-salsyx-no-cache` header bypass the cache so
/// the frontend's `no-store` fetches always get fresh data.
pub async fn cache_middleware(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let is_cacheable = should_cache(&req);
    let key = cache_key(&req);

    // Clients that pass no-cache always get a fresh response.
    let client_no_cache = req
        .headers()
        .get(header::CACHE_CONTROL)
        .map(|v| v.to_str().unwrap_or_default().contains("no-cache"))
        .unwrap_or(false)
        || req.headers().contains_key(CACHE_BYPASS_HEADER);

    if is_cacheable && !client_no_cache {
        if let Some(cached) = state.cache.get(&key) {
            return cached;
        }
    }

    let mut response = next.run(req).await;

    // Never cache streaming/download responses or errors.
    let can_cache = is_cacheable
        && !client_no_cache
        && response.status().is_success()
        && response
            .headers()
            .get(header::CONTENT_DISPOSITION)
            .is_none();

    if can_cache {
        // Extract the body once, cache it, and rebuild the response so the
        // client still gets the payload (Body is not Clone in axum 0.8).
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .cloned()
            .unwrap_or_else(|| HeaderValue::from_static("application/json"));
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = match axum::body::to_bytes(response.into_body(), usize::MAX).await {
            Ok(bytes) => bytes,
            Err(_) => return response_builder(status, headers, content_type, Body::empty()),
        };
        state
            .cache
            .store(key.clone(), content_type.clone(), bytes.to_vec());
        let mut cached = Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, content_type)
            .header("x-salsyx-cache", HeaderValue::from_static("MISS"))
            .body(Body::from(bytes))
            .expect("valid response");
        for (name, value) in headers {
            if let Some(name) = name {
                cached.headers_mut().append(name, value);
            }
        }
        cached.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=30"),
        );
        return cached;
    }

    // Short browser cache for read endpoints; downloads stay private.
    if is_cacheable {
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=30"),
        );
    }

    response
}

fn response_builder(
    status: axum::http::StatusCode,
    headers: axum::http::HeaderMap,
    content_type: HeaderValue,
    body: Body,
) -> Response {
    let mut response = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        .body(body)
        .expect("valid response");
    for (name, value) in headers {
        if let Some(name) = name {
            response.headers_mut().append(name, value);
        }
    }
    response
}

/// Only GET requests to stable, read-only paths get cached.
///
/// NOTE: this middleware is applied to the router nested at `/api/v1`, so
/// Axum strips the prefix and the paths here are relative to that mount
/// (i.e. `/stats`, not `/api/v1/stats`).
fn should_cache(req: &Request<Body>) -> bool {
    if req.method() != axum::http::Method::GET {
        return false;
    }
    let path = req.uri().path();
    ["/search", "/stats", "/stats/top", "/health"]
        .iter()
        .any(|p| path.starts_with(p))
}

/// Cache key: method + path + query.
fn cache_key(req: &Request<Body>) -> String {
    format!(
        "{} {}?{}",
        req.method(),
        req.uri().path(),
        req.uri().query().unwrap_or("")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get(path: &str) -> Request<Body> {
        Request::builder()
            .method("GET")
            .uri(path)
            .body(Body::empty())
            .unwrap()
    }

    #[test]
    fn cacheable_read_paths_match_without_api_prefix() {
        for path in [
            "/search?q=hello",
            "/search",
            "/stats",
            "/stats/top",
            "/health",
        ] {
            assert!(should_cache(&get(path)), "expected cacheable: {path}");
        }
    }

    #[test]
    fn non_cacheable_paths_and_methods_are_skipped() {
        assert!(!should_cache(&get("/repo/torvalds/linux")));
        assert!(!should_cache(&get("/archive/123/download")));
        assert!(!should_cache(&get("/admin/overview")));
        let post = Request::builder()
            .method("POST")
            .uri("/search")
            .body(Body::empty())
            .unwrap();
        assert!(!should_cache(&post));
    }
}
