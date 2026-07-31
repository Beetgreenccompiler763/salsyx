# Salsyx API server — multi-stage build.
#
# Uses a full rust image for compile and a slim distroless runtime so the
# final image is small and attack-surface-minimal. `git` is included because
# the crawler shells out to it; the API itself only needs the binary.

# ---------- build stage ----------
FROM rust:1.97-bookworm AS build
WORKDIR /app

# Cache dependencies: copy manifests first, build, then source.
COPY Cargo.toml Cargo.lock ./
COPY shared/Cargo.toml shared/
COPY backend/Cargo.toml backend/
COPY crawler/Cargo.toml crawler/

RUN mkdir -p shared/src backend/src crawler/src \
    && echo '// stub' > shared/src/lib.rs \
    && echo '// stub' > backend/src/lib.rs \
    && echo 'fn main() {}' > backend/src/main.rs \
    && echo 'fn main() {}' > crawler/src/main.rs \
    && cargo build --release -p salsyx-api -p salsyx-crawler || true

COPY shared shared
COPY backend backend
COPY crawler crawler

RUN touch shared/src/lib.rs backend/src/lib.rs backend/src/main.rs crawler/src/main.rs \
    && cargo build --release -p salsyx-api -p salsyx-crawler

# ---------- runtime stage ----------
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates git \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=build /app/target/release/salsyx-api /usr/local/bin/salsyx-api
COPY --from=build /app/target/release/salsyx-crawler /usr/local/bin/salsyx-crawler
COPY backend/config/default.toml /app/config/default.toml

EXPOSE 8080
ENV AH_APP__ENV=production \
    AH_SERVER__HOST=0.0.0.0 \
    AH_SERVER__PORT=8080

CMD ["salsyx-api"]
