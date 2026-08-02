# svm-indexer

A Solana/SVM (Fogo) account indexer. Given a set of RPC endpoints and on-chain programs, it fetches and persists account state over time and serves it over an API, so Umbra apps can read on-chain state from one shared, cached source instead of hitting the network RPC directly.

This README tracks the project's current state, layout, and near-term plans. It is not an exhaustive reference for every module.

## What is shipped

- **Admin HTTP API** (`/admin/*`) for managing indexing configuration:
  - `GET /admin` — validate the admin passkey (used by the panel login gate)
  - `POST /admin/rpcs` — register a new RPC endpoint
  - `GET /admin/rpcs` — list RPCs with pagination and their related programs
  - `GET /admin/rpcs/{id}` — get a single RPC with its related programs
  - `PATCH /admin/rpcs/{id}` — update RPC URLs
  - `DELETE /admin/rpcs/{id}` — remove an RPC
  - `POST /admin/programs` — register a program to index (by `rpc_id` or inline `rpc_url`)
  - `GET /admin/programs` — list programs with pagination and their RPC
  - `GET /admin/programs/{id}` — get a single program with its RPC
  - `PATCH /admin/programs/{id}` — update program settings
  - `DELETE /admin/programs/{id}` — remove a program
- **Admin authentication** via the `x-security` header (constant-time comparison).
- **Health check** at `GET /health`.
- **Admin panel** — a static HTMX single-page panel served from `static/panel` at `/`. It gates on the admin passkey and provides the navigation shell (Dashboard, RPCs, Programs, PDAs, History, Monitoring, Settings). Section bodies are placeholders pending wiring to the API.
- **PostgreSQL schema** for the configuration and account tables (see below).
- **Structured logging**: JSON `tracing` subscriber to stdout plus a panic hook.

## Database schema

| Table | Status | Purpose |
|---|---|---|
| `rpcs` | active | registered RPC endpoints (`http_url`, `ws_url`) |
| `programs` | active | programs to index, tied to an RPC (`address`, `name`, `last_processed_slot`, `last_processed_at`) |
| `program_accounts` | created | per-program account registry plus a cached latest state (`state`, `latest_slot`, `latest_tx_signature`, `first_seen_slot`) for fast current-state reads |
| `account_states` | in progress | per-slot state history for each account (migration still being fleshed out) |

Data is stored raw/program-agnostic; consumers decode it themselves.

## Layout

```
src/
  main.rs             # binary entrypoint: env, db, logging, serve
  lib.rs              # app(): Router assembly (admin nest, health, static panel fallback)
  core/               # state, error, response, logging
  middlewares/        # admin_guard (x-security header check)
  routes/
    root.rs           # /health + not-found
    admin/            # rpcs + programs handlers, admin passkey guard
    payloads/         # request bodies + pagination query
    responses/        # response shapes
  db/entities/        # SeaORM entities (programs, rpcs)
migration/            # SeaORM migrations
static/panel/         # HTMX admin panel (index.html, css, js, fragments)
tests/integration_tests/  # httpc-test suite against a real server + test db
stuff/                # design notes for the indexer/worker/streaming (reference, not wired in)
```

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
   - `APP_ENV` — optional, `development` (default) enables TRACE logging; `production` uses INFO

2. Run migrations:
   ```bash
   cargo run -p migration -- up
   ```

3. Start the server:
   ```bash
   cargo run
   ```

The admin panel is then available at `http://localhost:3000/`.

## Running tests

Integration tests live in `tests/integration_tests/` and use `httpc-test` against a real server instance backed by its own `<database>_test` database, which is created and migrated automatically. Tests reset their tables before each case and must run single-threaded:

```bash
cargo test --test integration_tests -- --test-threads=1
```

Set `TEST_DATABASE_URL` to override the test database; otherwise `DATABASE_URL` with a `_test` suffix on the database name is used.

To watch and rerun a specific test (requires `cargo install cargo-watch`):

```bash
cargo watch -q -c -w tests/ -x "test -q [test_func_name] -- --test-threads=1 --nocapture"
```

## REST client

`request.http` contains example calls for all current endpoints and can be used with the VS Code REST Client extension.

## What is coming next

- Block-by-block indexer that polls configured RPCs per program, extracts accounts touched by tracked programs, and persists current state plus history (backfill from a start slot, then stay live). Design notes in `stuff/`.
- Public API for querying an address's current state and full history.
- WebSocket push layer for live updates per address.
- Wiring the admin panel sections to the API, including per-program indexing progress.
