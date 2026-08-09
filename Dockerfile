# syntax=docker/dockerfile:1.7
FROM node:22-bookworm-slim AS frontend
WORKDIR /src/frontend
COPY frontend/package*.json ./
RUN npm install
COPY frontend/ ./
RUN npm run build

FROM rust:1-bookworm AS backend
RUN apt-get update \
    && apt-get install -y --no-install-recommends libsqlite3-dev pkg-config \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY Cargo.toml ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=backend /src/target/release/ashan-openrouter-manager /usr/local/bin/ashan-openrouter-manager
COPY --from=frontend /src/frontend/dist /app/web
RUN mkdir -p /data
ENV DATA_DIR=/data WEB_DIR=/app/web RUST_LOG=info
EXPOSE 8080
VOLUME ["/data"]
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 CMD curl -fsS http://127.0.0.1:8080/api/status >/dev/null || exit 1
ENTRYPOINT ["/usr/local/bin/ashan-openrouter-manager"]
