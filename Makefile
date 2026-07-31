# Salsyx developer convenience targets.
#
#   make dev          # run everything (db, api, crawler, web) via docker compose
#   make api          # run the API server locally (needs Postgres at AH_DATABASE__URL)
#   make crawler      # run the crawler locally
#   make web          # run the frontend dev server
#   make test         # run all Rust tests
#   make check        # clippy + fmt
#   make seed         # resolve a handful of popular repos to index locally
#   make db-reset     # drop + recreate the local schema (dev only)

.PHONY: dev api crawler web test check seed db-reset

dev:
	docker compose up --build

api:
	cargo run -p salsyx-api

crawler:
	cargo run -p salsyx-crawler

web:
	cd frontend && npm run dev

test:
	cargo test --all
	cd frontend && npm run typecheck

check:
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -D warnings

seed:
	@for repo in torvalds/linux facebook/react rust-lang/rust shadcn/ui vercel/next.js; do \
		echo "Resolving $$repo…"; \
		curl -s -X POST http://localhost:8080/api/v1/refresh -H "Content-Type: application/json" -d "{\"full_name\":\"$$repo\"}" > /dev/null; \
	done; \
	echo "Seeded. See /api/v1/stats"

db-reset:
	sudo -u postgres psql -d salsyx -c "DROP SCHEMA public CASCADE; CREATE SCHEMA public; GRANT ALL ON SCHEMA public TO salsyx; GRANT ALL ON SCHEMA public TO public;"
