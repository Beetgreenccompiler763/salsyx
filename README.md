# ArchiveHub

> **Nothing open-source should disappear forever.**

ArchiveHub is a search engine and preservation platform for public GitHub
repositories. Search any repository — if it still exists you get taken to
GitHub; if it was deleted, we restore it from the archive. Full git history,
checksums, and a beautiful futuristic interface.

![Rust](https://img.shields.io/badge/Rust-1.85+-e5734a?logo=rust)
![Next.js](https://img.shields.io/badge/Next.js-15-000000?logo=next.js)
![Postgres](https://img.shields.io/badge/Postgres-16-336791?logo=postgresql)
![License](https://img.shields.io/badge/license-MIT-blue)

---

## The primary journey

```
User searches → GitHub first
   ├─ exists?    → redirect to GitHub / download
   ├─ deleted?   → ArchiveHub database?
   │                ├─ archived?  → browse files / download the preserved bundle
   │                └─ no        → "This repository has not been archived."
   └─ rate-limited → graceful fallback to the archive database
```

## Architecture at a glance

```
frontend/   Next.js 15 + TypeScript + Tailwind v4 + Motion + GSAP + Lenis
            Bubbles, particles, glassmorphism, pixel accents, 60fps canvas.

backend/    Rust · Axum · Tokio · SQLx · Reqwest · Tower
            Clean Architecture: config / github / db / storage / service / routes.

crawler/    Independent Rust workers. Poll crawl_jobs (FOR UPDATE SKIP LOCKED),
            git clone → git bundle → SHA-256 → upload → record. Retry w/ backoff.

shared/     Domain types shared by all components (transport-agnostic).

docs/       architecture.md · deployment.md
```

**Storage strategy** — not ZIPs. The crawler produces **git bundles**: full
history, all refs, native git compression, natural cross-version dedup, one
immutable file per snapshot, SHA-256 verified on every read.

**Search** — Postgres `pg_trgm` (Neon free tier): substring + fuzzy matching
across name / owner / description, filtered by language, license, topics,
stars. Full-text (README) groundwork already in place.

**Queue** — durable `crawl_jobs` table (cross-process, restart-safe,
horizontally scalable) plus an in-memory event channel for fast feedback.

## Quick start

```bash
docker compose up -d db          # Postgres 16
cargo run -p archivehub-api      # API  → http://localhost:8080
cargo run -p archivehub-crawler  # workers
cd frontend && npm install && npm run dev   # UI → http://localhost:3000
```

Seed a few popular repositories:

```bash
make seed
```

Then open http://localhost:3000 and search `torvalds/linux`. See
[docs/deployment.md](docs/deployment.md) for full local + production setup.

## API (OpenAPI 3.1)

```
GET  /api/v1/health
GET  /api/v1/search            ?q=…&language=…&min_stars=…&sort=…
GET  /api/v1/repo/{owner}/{repo}
GET  /api/v1/archive/{id}
GET  /api/v1/download/{id}
GET  /api/v1/stats
GET  /api/v1/stats/top
POST /api/v1/archive           # request preservation of owner/repo
POST /api/v1/refresh
```

The live document is served at `/openapi.json`.

## Tech stack

| Layer      | Choice                                                    |
| ---------- | --------------------------------------------------------- |
| Backend    | Rust, Axum, Tokio, SQLx, Reqwest, Tracing, Tower          |
| Frontend   | Next.js, TypeScript, TailwindCSS, Motion, GSAP, Anime.js, Lenis, React Three Fiber, Lucide |
| Database   | Neon PostgreSQL (free tier)                               |
| Storage    | Cloudflare R2                                             |
| Deploy     | Fly.io (API + crawler), Cloudflare Pages (frontend)       |
| CI/CD      | GitHub Actions                                            |
| Monitoring | Sentry (feature-gated), UptimeRobot                       |

## Design language

GitHub × Linear × Raycast × Arc × Vercel × Apple × Nothing Phone × Cyberpunk
× Pixel Art × Minimalism. Dark first. Glass cards, soft glowing borders,
mouse parallax, micro-interactions everywhere — the landing page is a field
of hundreds of physics-driven GitHub user bubbles that you can pop open into
profile modals.

## Roadmap (architecture ready)

Automatic pre-deletion archiving · Software Heritage / Wayback / archive.org
integration · public API keys · user accounts & favorites · repository
history / version snapshots · trending deleted repositories · AI / semantic
code search · global deduplication · custom long-term archive format ·
distributed storage · analytics dashboard.

## License

MIT — see [LICENSE](LICENSE).
