# Salsyx crawler workers — self-contained multi-stage build.
# The crawler shells out to `git` (clone --bare / bundle create), so the
# runtime image includes git.

# ---------- build stage ----------
FROM rust:1.97-bookworm AS build
WORKDIR /app

# Cache dependencies: copy manifests first, build, then source.
COPY Cargo.toml Cargo.lock ./
COPY shared/Cargo.toml shared/
COPY backend/Cargo.toml backend/
COPY crawler/Cargo.toml crawler/
COPY aahl/Cargo.toml aahl/

RUN mkdir -p shared/src backend/src crawler/src aahl/src \
    && echo '// stub' > shared/src/lib.rs \
    && echo '// stub' > backend/src/lib.rs \
    && echo 'fn main() {}' > backend/src/main.rs \
    && echo 'fn main() {}' > crawler/src/main.rs \
    && echo '// stub' > aahl/src/lib.rs \
    && cargo build --release -p salsyx-api -p salsyx-crawler || true

COPY shared shared
COPY backend backend
COPY crawler crawler
COPY aahl aahl

RUN touch shared/src/lib.rs backend/src/lib.rs backend/src/main.rs crawler/src/main.rs aahl/src/lib.rs \
    && cargo build --release -p salsyx-api -p salsyx-crawler

# ---------- runtime stage ----------
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates git \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=build /app/target/release/salsyx-crawler /usr/local/bin/salsyx-crawler
COPY backend/config/default.toml /app/config/default.toml

ENV AH_APP__ENV=production

CMD ["salsyx-crawler"]
