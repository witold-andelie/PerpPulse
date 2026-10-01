FROM rust:1.95-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY docs/methodology.json ./docs/methodology.json
COPY deploy/serving-schema.sql ./deploy/serving-schema.sql
COPY web ./web
RUN cargo build --locked --release -p perppulse

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home-dir /app perppulse
WORKDIR /app
COPY --from=builder /app/target/release/perppulse /usr/local/bin/perppulse
COPY fixtures ./fixtures
USER 10001
EXPOSE 8080
ENTRYPOINT ["perppulse"]
CMD ["serve", "fixtures/golden/open-position-as-of.json", "--bind", "0.0.0.0:8080"]
