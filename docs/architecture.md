# Salsyx — Architecture

> "Nothing open-source should disappear forever."

Salsyx is a search engine and preservation platform for public GitHub
repositories. This document describes the system architecture, the reasoning
behind each decision, and the roadmap that the codebase is designed to support.

---

## 1. High-level system

```
┌──────────────┐   HTTPS    ┌─────────────────┐   HTTPS   ┌──────────────────┐
│   Browser     │ ────────► │  Next.js (FE)   │ ────────► │  Rust API (BE)   │
│  (bubbles,    │ ◄──────── │  Cloudflare     │ ◄──────── │  Fly.io/Railway  │
│  search, …)   │           │  Pages          │  /api/v1  │  port 8080       │
└──────────────┘            └─────────────────┘           └────────┬─────────┘
                                                                    │
                                  ┌─────────────────────────────────┼──────────────┐
                                  │                                 │              │
                          ┌───────▼───────┐               ┌─────────▼─────┐  ┌─────▼─────────┐
                          │   Neon Postgres│               │  Cloudflare R2 │  │  GitHub API    │
                          │   (metadata,   │               │  (archive blobs │  │  (live source) │
                          │   search,      │               │   + checksums)  │  └───────────────┘
                          │   jobs)        │               └─────────────────┘
                          └───────▲───────┘
                                  │
                          ┌───────┴───────────────────────────┐
                          │           Crawler workers          │
                          │   (independent Rust processes,     │
                          │    poll crawl_jobs, git clone,     │
                          │    bundle, checksum, upload)       │
                          └────────────────────────────────────┘
```

**Key property:** every component talks to the *database* and *storage* only
through well-defined modules. The API and crawler are separate processes that
share no runtime state — they coordinate through the `crawl_jobs` table.

---

## 2. Repository flow (the primary journey)

```
User searches `owner/repo`
            │
            ▼
   1. GET /repo/{owner}/{repo}
            │
            ├─ GitHub API 200 ────────► "live"  → redirect to GitHub / download
            │
            ├─ GitHub API 404 ─────────► is the repo in our DB?
            │                                ├─ yes + has archive ──► "archived" → browse/download bundle
            │                                ├─ yes + no archive ──► "not_archived"
            │                                └─ no ────────────────► "not_found"
            │
            └─ GitHub rate-limited ─────► fall back to DB lookup (graceful degradation)
```

Every response includes `status`, `source`, the full repository metadata, the
archive record (when present), and a ready-to-use download URL.

---

## 3. Clean Architecture (module boundaries)

```
shared/         Domain types shared by every component (archive, repository,
                events, search, errors). Transport- and storage-agnostic.
                ── depends on nothing else in the repo ──

backend/        The API server.
  src/config.rs     Configuration (env + TOML), one place for all knobs.
  src/github.rs     GitHub REST client (rate-limit aware).
  src/db.rs         Repository pattern — all SQL lives here.
  src/storage.rs    Storage trait + local + R2 providers.
  src/service.rs    Domain service: repo resolution orchestration.
  src/routes/*.rs   HTTP layer only: parse request, call service, serialize.
  src/state.rs      Shared AppState handed to handlers via Axum.

crawler/        Independent worker processes.
  src/jobs.rs      Claim/complete/fail/enqueue crawl_jobs (retry w/ backoff).
  src/pipeline.rs  git clone → bundle → checksum → upload → record.
  ── shares the database and storage through salsyx-api's modules ──

frontend/       Next.js app (App Router, TypeScript, Tailwind v4).
  lib/api.ts       Typed client for the REST API.
  components/      BubbleField, ParticleField, SearchBar, glass cards, …
```

**Rules**
- HTTP handlers never touch SQL or storage directly.
- Domain types (`shared/`) never depend on a web framework or database driver.
- The crawler is a *consumer* of `salsyx-api`'s low-level modules; it is
  not required to run for the API to serve live lookups.
- GraphQL can be added behind `/graphql` calling the same `service.rs`
  functions — the services don't know what transport is in front of them.

---

## 4. Database schema

Tables (see `backend/migrations/`):

| Table            | Purpose                                              |
| ---------------- | ---------------------------------------------------- |
| `owners`         | GitHub users/organizations (normalized away from repos) |
| `repositories`   | Central entity: metadata, visibility, deletion state |
| `archives`       | Immutable point-in-time snapshots (checksum, storage) |
| `downloads`      | Per-archive download analytics                       |
| `repo_stats`     | Daily stars/forks/… snapshots for trends             |
| `crawl_jobs`     | Durable worker queue (claim/retry/backoff)           |
| `repo_documents` | README + description for future full-text search     |

Key design choices:

- **UUID v4 primary keys** — no sequential IDs, no information leakage, no
  single-writer bottleneck; sharding/multi-writer stays possible.
- **Status enums are TEXT + CHECK constraints** — the canonical set lives in
  the schema, not scattered in code.
- **Trigram GIN indexes** (`pg_trgm`) power substring + fuzzy search on the
  Neon free tier — no external search service needed at launch.
- **`crawl_jobs` uses `FOR UPDATE SKIP LOCKED`** — concurrent workers can
  claim jobs without double-processing.

---

## 5. Storage strategy (why git bundles, not ZIPs)

The spec says "do not simply store ZIP files". The pipeline produces
**git bundles** instead:

| Property                | GitHub ZIP                    | Salsyx git bundle            |
| ----------------------- | ----------------------------- | -------------------------------- |
| History                 | ❌ current tree only          | ✅ all commits, all refs         |
| `.git` metadata         | ❌                            | ✅                                |
| Compression             | zip (fine)                    | git's zlib + delta packs         |
| Dedup across versions   | ❌ every snapshot is a full ZIP | ✅ identical objects pack once |
| Single immutable file   | ✅                            | ✅ (checksum-friendly)           |
| Restore                 | unzip                         | `git clone bundle.bundle`        |

The pipeline (`crawler/src/pipeline.rs`) is structured so the "produce the
blob" step can be swapped for a future custom long-term format without
touching storage, jobs, or API code.

Every blob is **SHA-256 hashed at rest**, the hash is stored next to the
object key, and *every* read path re-verifies (`Storage::get` fails on
checksum mismatch). "Verify before trusting" is a hard invariant.

### AAHL — the custom long-term format

The `aahl` crate (`aahl/`) is the content-addressed archive format for
long-term preservation. The crawler can produce AAHL snapshots instead of
bundles (`AH_APP__CRAWLER_FORMAT=aahl`):

- **Content-defined chunking** (buzhash) splits files into variable-size
  chunks; each chunk is SHA-256-addressed. Identical chunks — across files,
  across repositories, across snapshots — are stored once.
- **Incremental snapshots** link a manifest to its `parent`, so a re-crawl
  only adds changed chunks.
- **Zstandard compression** per chunk, with the digest computed over the
  *uncompressed* bytes so dedup is independent of encoding.
- **Small manifests**: a snapshot is a versioned, checksummed JSON index of
  entries + chunk digests; the checksum stored in `archives.checksum` is the
  manifest digest, and the manifest blob itself lives at
  `archives/{repo_id}/{archive_id}.aahl`.
- **Verify before trusting, again**: `aahl::decode::read_file` re-hashes every
  chunk against the manifest before emitting bytes; `aahl::decode::extract`
  reconstructs the full tree.
- Chunks persist through the same `Storage` abstraction via
  `salsyx_api::aahl::StorageChunkStore` under the `aahl/{digest}` key
  namespace, so local dev and R2 use identical code.

Both formats are browsable through the same API: `/archive/{id}/tree` serves
the preserved listing and `/archive/{id}/blob` streams any file. For AAHL
archives those routes decode straight from the manifest + chunk store.

### External archive chain

When GitHub returns 404 for a repository, the resolver now walks a chain of
external preservation providers (`backend/src/providers/`) behind a common
`ArchiveProvider` trait:

| Provider                | What it provides                                   | Config key            |
| ----------------------- | -------------------------------------------------- | --------------------- |
| **Software Heritage**   | origin/visit + snapshot metadata, status + commit  | `software_heritage`   |
| **Wayback Machine**     | archived snapshots of the GitHub repo page         | `wayback`             |
| **Archive.org**         | item lookup (metadata + download count)            | `archive_org`         |

Providers are consulted in order, best-effort: a provider failure is logged
and the chain continues. Individual providers can be disabled with
`AH_PROVIDERS__DISABLED=["archive_org", ...]`. Results are folded into the
repo response as `external_archives` so the frontend can link users out to a
recovery copy when Salsyx has not archived the repo itself.

---

## 6. Search engine

Today: Postgres `pg_trgm` — `ILIKE` substring + `similarity()` ranking across
`full_name`, `name`, `owner login`, and `description`, filtered by language /
license / topics / stars, sorted by stars / forks / updated / relevance.

The `repo_documents` table + tsvector trigger already lay the groundwork for
full-text (README) search, and the `search` route isolates its query behind
`db::search_repositories`, so swapping in Tantivy/Meilisearch/Typesense later
is a contained change.

---

## 7. Queue & workers (future-ready)

- The **durable** coordination mechanism is the `crawl_jobs` table
  (works across processes, survives restarts, supports horizontal scaling).
- The **in-memory** `EventQueue` (async-channel) exists for fast
  single-process feedback and is type-compatible (`shared/src/events.rs`)
  with swapping in Redis streams / Postgres LISTEN-NOTIFY later.
- Retries use exponential backoff (`2^attempts` minutes) up to `max_attempts`,
  then jobs go `dead` and stop retrying.

---

## 8. Deployment

| Component | Host           | Notes                                        |
| --------- | -------------- | -------------------------------------------- |
| Frontend  | Cloudflare Pages | `@cloudflare/next-on-pages`; static + edge  |
| API       | Fly.io         | `backend/fly.toml`, 1 machine (scales out)   |
| Crawler   | Fly.io         | `crawler/fly.toml`, `flyctl scale count N`    |
| Database  | Neon (free)    | Postgres 16; migrations auto-run on boot     |
| Storage   | Cloudflare R2  | S3-compatible; local provider for dev        |
| CI/CD     | GitHub Actions | `ci.yml` (lint/test) + `deploy.yml`          |
| Monitoring| Sentry + UptimeRobot | sentry feature-gated; `/health` endpoint |

---

## 9. Roadmap hooks (architecture already supports)

- **Automatic archiving before deletion** → crawler polls trending repos and
  refreshes periodically (schema: `last_checked_at`, `crawl_jobs`).
- **Software Heritage / Wayback / archive.org integration** → new `source`
  enum values + resolver fallback chain.
- **Public API / API keys** → rate-limit middleware layer + `users` table.
- **Favorites / accounts** → `users`, `favorites` tables.
- **Repo history / version snapshots** → multiple `archives` rows per repo
  already supported (`idx_archives_repository`).
- **Trending deleted repositories** → `repo_stats` + `deleted_at` query.
- **AI code search / semantic search** → embeddings column + vector index.
- **Custom archive format / compression research** → new
  `CompressionMethod` variant + `pipeline` step swap.
- **Distributed storage** → `Storage` trait; keys already namespaced by repo.

---

## 10. Performance notes

- Frontend: server components where possible, canvas-based particle/bubble
  systems (no DOM churn), `next/image` for avatars, lazy-loaded modals.
- Backend: streaming bodies where sensible, gzip middleware, connection
  pooling, request IDs, structured JSON logs, graceful shutdown.
- The API is stateless — horizontal scaling is trivial.
