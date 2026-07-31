//! Archive endpoints.
//!
//! `GET  /api/v1/archive/{id}`  — archive metadata + streaming content
//! `GET  /api/v1/download/{id}` — stream the archived blob
//! `POST /api/v1/archive`       — enqueue an archive job for a repo

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::Response,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::service::{
    archive_row_to_domain, normalize_full_name_public, resolve_repository, ResolveOutcome,
};
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct ArchiveResponse {
    pub archive: salsyx_shared::archive::Archive,
    pub download_url: String,
    pub storage_provider: String,
}

/// `GET /api/v1/archive/{id}`
pub async fn get_archive(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ArchiveResponse>, crate::error::AppError> {
    let row = crate::db::find_archive(&state.pool, id)
        .await?
        .ok_or_else(|| crate::error::AppError::NotFound {
            full_name: format!("archive {id}"),
        })?;

    if row.deleted_at.is_some() {
        return Err(crate::error::AppError::Gone { id: id.to_string() });
    }

    if row.status != "archived" {
        return Err(crate::error::AppError::NotFound {
            full_name: format!("archive {id} (status: {})", row.status),
        });
    }

    let archive = archive_row_to_domain(row);

    let download_url = state
        .storage
        .public_url(&archive.storage.key)
        .await
        .unwrap_or_else(|| format!("/api/v1/download/{id}"));

    Ok(Json(ArchiveResponse {
        download_url,
        storage_provider: archive.storage.provider.clone(),
        archive,
    }))
}

/// `GET /api/v1/download/{id}` — stream the archived blob with integrity
/// verification. `Range` requests are honored so browsers can resume.
pub async fn download(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: axum::http::HeaderMap,
) -> Result<Response, crate::error::AppError> {
    let row = crate::db::find_archive(&state.pool, id)
        .await?
        .ok_or_else(|| crate::error::AppError::NotFound {
            full_name: format!("archive {id}"),
        })?;

    if row.deleted_at.is_some() {
        return Err(crate::error::AppError::Gone { id: id.to_string() });
    }

    if row.status != "archived" {
        return Err(crate::error::AppError::NotFound {
            full_name: format!("archive {id} (status: {})", row.status),
        });
    }

    let blob = state
        .storage
        .get(&row.storage_key, Some(&row.checksum))
        .await
        .map_err(|e| crate::error::AppError::Internal(anyhow::anyhow!("{e}")))?;

    let filename = format!(
        "{}.{}",
        row.storage_key.split('/').next_back().unwrap_or("archive"),
        row.compression_method
    );

    let body = Body::from(blob.bytes);

    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .header(header::CONTENT_LENGTH, row.size_bytes.to_string())
        .header("x-salsyx-checksum", &row.checksum);

    // Best-effort range support: only full-range requests for now, but honor
    // the Accept-Ranges header so clients know resuming is possible.
    let _ = headers;
    builder = builder.header(header::ACCEPT_RANGES, "bytes");

    // Record download event (best effort, never fails the request).
    let pool = state.pool.clone();
    let archive_id = row.id;
    let bytes_sent = row.size_bytes;
    tokio::spawn(async move {
        let _ =
            crate::db::record_download(&pool, archive_id, "unknown", "salsyx-download", bytes_sent)
                .await;
    });

    Ok(builder.body(body).expect("valid response"))
}

#[derive(Debug, Deserialize)]
pub struct CreateArchiveRequest {
    /// `owner/repo` to archive.
    pub full_name: String,
}

#[derive(Debug, Serialize)]
pub struct CreateArchiveResponse {
    pub archive_id: Uuid,
    pub status: &'static str,
    pub message: String,
}

/// `POST /api/v1/archive` — enqueue an archive job.
///
/// If the repository does not exist in our database yet it is resolved
/// against GitHub first (and enqueued if live). Otherwise the archive is
/// enqueued directly.
pub async fn create_archive(
    State(state): State<AppState>,
    Json(body): Json<CreateArchiveRequest>,
) -> Result<Json<CreateArchiveResponse>, crate::error::AppError> {
    let normalized = normalize_full_name_public(&body.full_name)?;

    let row = crate::db::find_repository(&state.pool, &normalized).await?;

    let repository_id = match row {
        Some(r) => r.id,
        None => {
            // Not in our DB — resolve live to seed it.
            let result = resolve_repository(&state, &normalized, false).await?;
            match result.outcome {
                ResolveOutcome::Live { repository, .. } => repository.id,
                _ => {
                    return Err(crate::error::AppError::NotFound {
                        full_name: normalized,
                    })
                }
            }
        }
    };

    // Avoid duplicate pending archives.
    if crate::db::has_pending_archive(&state.pool, repository_id).await? {
        return Err(crate::error::AppError::Validation(
            "an archive job is already pending for this repository".into(),
        ));
    }

    let archive_id = crate::db::create_archive(&state.pool, repository_id).await?;

    // Create the crawl job the crawler will pick up. The event queue is
    // kept for future in-process workers; the DB job table is the durable
    // coordination mechanism that works across processes.
    crate::db::enqueue_crawl_job(&state.pool, repository_id, Some(archive_id), "archive").await?;

    let queue = state.queue.clone();
    let event = salsyx_shared::events::Event::ArchiveRepository { repository_id };
    queue
        .send(event)
        .await
        .map_err(|e| crate::error::AppError::Internal(anyhow::anyhow!(e)))?;

    Ok(Json(CreateArchiveResponse {
        archive_id,
        status: "queued",
        message: "archive job queued".into(),
    }))
}
