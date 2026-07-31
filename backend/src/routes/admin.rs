//! Admin overview endpoints, protected by `AH_SERVER__ADMIN_TOKEN`.
//!
//! The dashboard is a thin read-only surface over the operational state:
//! repository/archive counts, storage usage, crawl queue health, and the
//! configured provider chain. Everything here is best-effort diagnostics —
//! failures degrade to `null` rather than erroring the whole response.

use axum::{extract::State, http::HeaderMap, Json};
use serde::Serialize;

use crate::error::AppError;
use crate::state::AppState;

/// `GET /api/v1/admin/overview` — operational snapshot for the dashboard.
pub async fn overview(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<OverviewResponse>, AppError> {
    let token = state
        .config
        .server
        .admin_token
        .as_deref()
        .filter(|t| !t.is_empty());
    let Some(token) = token else {
        return Err(AppError::AdminDisabled);
    };
    authorize(&headers, token)?;

    let counts: Option<Counts> = match sqlx::query_as::<_, Counts>(
        r#"
        SELECT
            (SELECT count(*) FROM repositories)::bigint AS total_repositories,
            (SELECT count(*) FROM repositories WHERE is_deleted)::bigint AS deleted_repositories,
            (SELECT count(DISTINCT repository_id) FROM archives WHERE status = 'archived')::bigint AS archived_repositories,
            (SELECT count(*) FROM archives WHERE status = 'archived')::bigint AS total_archives,
            (SELECT COALESCE(sum(size_bytes), 0) FROM archives WHERE status = 'archived')::bigint AS total_storage_bytes
        "#,
    )
    .fetch_one(&state.pool)
    .await
    {
        Ok(counts) => Some(counts),
        Err(e) => {
            tracing::warn!(error = %e, "admin counts query failed");
            None
        }
    };

    let job_breakdown: Vec<JobCount> = sqlx::query_as(
        r#"
        SELECT job_type, status, count(*)::bigint
        FROM crawl_jobs
        GROUP BY job_type, status
        ORDER BY count(*) DESC
        "#,
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let pending_jobs: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM crawl_jobs WHERE status IN ('pending', 'claimed')",
    )
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    let recent_archives: Vec<RecentArchive> = sqlx::query_as(
        r#"
        SELECT a.id, r.full_name, a.status, a.compression_method, a.size_bytes, a.archived_at
        FROM archives a
        JOIN repositories r ON r.id = a.repository_id
        ORDER BY a.archived_at DESC
        LIMIT 10
        "#,
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let providers: Vec<ProviderInfo> = state
        .providers
        .iter()
        .map(|p| ProviderInfo {
            name: p.name().to_string(),
            enabled: true,
        })
        .collect();
    let disabled: Vec<ProviderInfo> = state
        .config
        .providers
        .disabled
        .iter()
        .map(|d| ProviderInfo {
            name: d.clone(),
            enabled: false,
        })
        .collect();

    Ok(Json(OverviewResponse {
        generated_at: chrono::Utc::now().to_rfc3339(),
        counts,
        pending_jobs,
        job_breakdown,
        recent_archives,
        providers: {
            let mut all = providers;
            all.extend(disabled);
            all
        },
        storage: StorageInfo {
            provider: state.storage.provider_name().to_string(),
            key_namespace: crate::aahl::CHUNK_PREFIX.to_string(),
        },
        formats: vec![
            FormatInfo {
                name: "git_bundle".to_string(),
                default: state.config.app.crawler_format != "aahl",
            },
            FormatInfo {
                name: "aahl".to_string(),
                default: state.config.app.crawler_format == "aahl",
            },
        ],
    }))
}

#[derive(Debug, sqlx::FromRow, Serialize)]
struct Counts {
    total_repositories: i64,
    deleted_repositories: i64,
    archived_repositories: i64,
    total_archives: i64,
    total_storage_bytes: i64,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
struct JobCount {
    job_type: String,
    status: String,
    count: i64,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
struct RecentArchive {
    id: uuid::Uuid,
    full_name: String,
    status: String,
    compression_method: String,
    size_bytes: Option<i64>,
    archived_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize)]
struct ProviderInfo {
    name: String,
    enabled: bool,
}

#[derive(Debug, Serialize)]
struct StorageInfo {
    provider: String,
    key_namespace: String,
}

#[derive(Debug, Serialize)]
struct FormatInfo {
    name: String,
    default: bool,
}

#[derive(Debug, Serialize)]
pub struct OverviewResponse {
    generated_at: String,
    counts: Option<Counts>,
    pending_jobs: i64,
    job_breakdown: Vec<JobCount>,
    recent_archives: Vec<RecentArchive>,
    providers: Vec<ProviderInfo>,
    storage: StorageInfo,
    formats: Vec<FormatInfo>,
}

/// Constant-time-ish bearer-token check for the admin endpoints.
fn authorize(headers: &HeaderMap, token: &str) -> Result<(), AppError> {
    let provided = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    if provided != Some(token) {
        return Err(AppError::Unauthorized);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorize_accepts_matching_bearer() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer sekrit".parse().unwrap());
        assert!(authorize(&headers, "sekrit").is_ok());
    }

    #[test]
    fn authorize_rejects_missing_or_wrong() {
        let mut headers = HeaderMap::new();
        assert!(authorize(&headers, "sekrit").is_err());

        headers.insert("authorization", "Bearer wrong".parse().unwrap());
        assert!(authorize(&headers, "sekrit").is_err());

        headers.insert("authorization", "Basic dXNlcjpwYXNz".parse().unwrap());
        assert!(authorize(&headers, "sekrit").is_err());
    }
}
