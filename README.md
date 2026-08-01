# svm-indexer

A Solana/SVM account indexer. Given a set of RPC endpoints and on-chain programs, it fetches and persists account state over time.

## What is shipped

- **Admin HTTP API** (`/admin/*`) for managing indexing configuration:
  - `POST /admin/rpcs` — register a new RPC endpoint
  - `GET /admin/rpcs` — list RPCs with pagination
  - `GET /admin/rpcs/{id}` — get a single RPC with its related programs
  - `PATCH /admin/rpcs/{id}` — update RPC URLs
  - `DELETE /admin/rpcs/{id}` — remove an RPC
  - `POST /admin/programs` — register a program to index
  - `GET /admin/programs` — list programs with pagination
  - `GET /admin/programs/{id}` — get a single program with its RPC
  - `PATCH /admin/programs/{id}` — update program settings
  - `DELETE /admin/programs/{id}` — remove a program
- **Admin authentication** via the `x-security` header.
- **Health check** at `GET /`.
- **PostgreSQL schema** for RPCs, programs, accounts, account state history, and indexing errors (the admin tables are active; the rest are prepared for the indexer).

## Tech stack

- Rust 2024 edition
- Axum web framework
- SeaORM + PostgreSQL
- JSON tracing subscriber + panic hook

## Running locally

1. Copy `.env.example` to `.env` and fill in:
   - `DATABASE_URL` — PostgreSQL connection string
   - `ADMIN_PASSKEY` — secret used for the `x-security` header
   - `APP_PORT` — optional, defaults to `3000`

2. Run migrations:
   ```bash
   cargo run -p migration -- up
   ```

3. Start the server:
   ```bash
   cargo run
   ```

## Running tests

Integration tests live in `tests/integration_tests/` and use `httpc-test` against a real server instance backed by the configured database. Tests reset their tables before each case and must run single-threaded:

```bash
cargo test --test integration_tests -- --test-threads=1
```

To watch and rerun a specific test (requires `cargo install cargo-watch`):

```bash
cargo watch -q -c -w tests/ -x "test -q [test_func_name] -- --test-threads=1 --nocapture"
```

## REST client

`request.http` contains example calls for all current endpoints and can be used with the VS Code REST Client extension.

## What is coming next

- Block-by-block indexer that polls configured RPCs, extracts accounts touched by tracked programs, and persists current state plus history.
- Public API for querying account state.
- WebSocket push layer for live updates.
