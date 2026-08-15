FROM rust:1.97 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
WORKDIR /app

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    libssl3 \
    curl \
    ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/svm-indexer /app/svm-indexer

COPY --from=builder /app/static /app/static

RUN chmod +x /app/svm-indexer

EXPOSE 3000

CMD ["./svm-indexer"]
