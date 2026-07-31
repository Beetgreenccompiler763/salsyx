# Salsyx — Development & Deployment Guide

## Prerequisites

- Rust 1.85+ (`rustup`)
- Node.js 20+ and npm
- Postgres 16 (or Docker)
- `git`

## Local development

### 1. Database

```bash
docker compose up -d db          # or use your own Postgres
```

Create the database (if not using docker compose):

```sql
CREATE USER salsyx WITH PASSWORD 'salsyx' CREATEDB;
CREATE DATABASE salsyx OWNER salsyx;
```

### 2. Backend (API)

```bash
cp backend/.env.example backend/.env
cargo run -p salsyx-api
```

Migrations apply automatically on startup (`AH_DATABASE__AUTO_MIGRATE=true`).

Smoke test:

```bash
curl localhost:8080/api/v1/health        # {"status":"ok",...}
curl localhost:8080/api/v1/repo/torvalds/linux
curl "localhost:8080/api/v1/search?q=linux"
```

### 3. Crawler (workers)

```bash
cargo run -p salsyx-crawler
```

The crawler polls `crawl_jobs`. Enqueue work with:

```bash
curl -X POST localhost:8080/api/v1/archive \
  -H "Content-Type: application/json" \
  -d '{"full_name":"octocat/Hello-World"}'
```

### 4. Frontend

```bash
cd frontend
cp .env.example .env.local
npm install
npm run dev        # http://localhost:3000
```

The dev server proxies `/api/*` → `API_ORIGIN` (default `http://localhost:8080`).

### 5. Everything at once

```bash
make dev
```

---

## Tests & quality gates

```bash
make check          # rustfmt + clippy (-D warnings)
make test           # cargo test + frontend typecheck
```

CI (`.github/workflows/ci.yml`) runs these against a throwaway Postgres.

---

## Configuration reference

All backend config is read from `backend/config/default.toml` and overridden
by `AH_`-prefixed env vars (`AH_SERVER__PORT=9000`, `AH_DATABASE__URL=…`).

| Variable | Purpose | Default |
| -------- | ------- | ------- |
| `AH_APP__ENV` | `development` \| `production` | `development` |
| `AH_SERVER__PORT` | HTTP port | `8080` |
| `AH_SERVER__ALLOWED_ORIGIN` | CORS origin | `http://localhost:3000` |
| `AH_DATABASE__URL` | Postgres URL | local compose db |
| `AH_GITHUB__TOKEN` | PAT (5000 req/h) or blank (60 req/h) | blank |
| `AH_GITHUB__WEBHOOK_SECRET` | HMAC secret for `POST /webhook/github` | blank (verification off) |
| `AH_STORAGE__PROVIDER` | `local` \| `r2` | `local` |
| `AH_STORAGE__R2_*` | R2 endpoint/bucket/keys | — |
| `AH_SENTRY_DSN` | Sentry DSN (needs `--features sentry`) | — |

---

## Production deployment

### Backend + crawler → Fly.io

```bash
cd backend
flyctl launch
flyctl secrets set AH_DATABASE__URL=postgres://…@…/salsyx
flyctl secrets set AH_GITHUB__TOKEN=…
flyctl secrets set AH_GITHUB__WEBHOOK_SECRET=…   # enables signature verification
flyctl secrets set AH_STORAGE__PROVIDER=r2 AH_STORAGE__R2_ACCOUNT_ID=… …
flyctl deploy
```

Crawler (independent machine, no public port):

```bash
cd crawler
flyctl launch
flyctl secrets set AH_DATABASE__URL=…
flyctl scale count 2
```

### Frontend → Cloudflare Pages

```bash
cd frontend
npx @cloudflare/next-on-pages
npx wrangler pages deploy .vercel/output/static --project-name=salsyx
```

Set `API_ORIGIN` to the deployed API URL. The API rewrite in
`next.config.mjs` sends `/api/*` to the Rust backend.

### Database → Neon free tier

Provision a Neon project, copy the pooled connection string into
`AH_DATABASE__URL` on both API and crawler. Migrations run automatically on
API boot; disable auto-migrate for the crawler to avoid races
(`AH_DATABASE__AUTO_MIGRATE=false`).

### Storage → Cloudflare R2

1. Create a bucket (e.g. `salsyx`).
2. Create an R2 API token (S3-compatible).
3. Set the `AH_STORAGE__R2_*` secrets on API + crawler.
4. Optionally set `AH_STORAGE__R2_PUBLIC_BASE_URL` to a public bucket URL so
   downloads can bypass the API.

---

## Monitoring

- **Sentry**: build with `cargo build --features sentry` and set `AH_SENTRY_DSN`.
  ERROR spans become exceptions; WARN becomes events; 10% trace sampling.
- **UptimeRobot**: point a monitor at `GET /api/v1/health` (returns 200 while
  the process is up; `database` field reports readiness).

---

## API reference

OpenAPI 3.1 document served at `/openapi.json` (also committed at
`backend/openapi.json`). Highlights:

```
GET  /api/v1/health
GET  /api/v1/search            ?q=linux&language=Rust&min_stars=100&sort=stars
GET  /api/v1/repo/{owner}/{repo}
GET  /api/v1/archive/{id}
GET  /api/v1/download/{id}     # streams the archived blob (checksum-verified)
GET  /api/v1/stats
GET  /api/v1/stats/top
POST /api/v1/archive           # {"full_name":"owner/repo"}
POST /api/v1/refresh           # {"full_name":"owner/repo"}
```
