FROM rust:1.88-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
COPY templates ./templates
COPY resources ./resources
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/over-there /usr/local/bin/over-there
COPY static ./static
RUN mkdir -p /app/data && useradd -r -u 10001 -d /app routeuser && chown -R routeuser:routeuser /app/data
USER routeuser
ENV DATABASE_URL=sqlite://data/over-there.db STORAGE_DIR=data/files LISTEN_ADDR=0.0.0.0:3000
EXPOSE 3000
CMD ["over-there"]
