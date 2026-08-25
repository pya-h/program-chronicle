FROM rust:1.97 AS builder
WORKDIR /app
COPY . .
RUN cargo build --release --workspace

FROM debian:bookworm-slim
WORKDIR /app

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    libssl3 \
    curl \
    ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/svm-indexer /app/svm-indexer

COPY --from=builder /app/target/release/migration /app/migration

COPY --from=builder /app/static /app/static

RUN chmod +x /app/svm-indexer /app/migration

EXPOSE 3000

# `migration` is the migrator crate's own binary; it reads DATABASE_URL and exits non-zero on
# failure, so a bad schema stops the container instead of starting the indexer against it.
CMD ["sh", "-c", "./migration up && exec ./svm-indexer"]
