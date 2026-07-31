# ArchiveHub crawler — runtime stage only (reuses the API image's build
# outputs by copying from the backend build stage defined in backend/Dockerfile).
#
# NOTE: Build both via `docker compose build` (docker-compose.yml) which
# builds the full workspace once.

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates git \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY backend/config/default.toml /app/config/default.toml

ENV AH_APP__ENV=production
CMD ["/usr/local/bin/archivehub-crawler"]
