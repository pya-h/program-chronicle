# svm-indexer

A Solana/SVM (Fogo) account indexer. Given a set of RPC endpoints and on-chain programs, it fetches and persists account state, transactions, and program events over time and serves them over an HTTP + SSE API, so Umbra apps can read on-chain state from one shared, cached source instead of hitting the network RPC directly.

This README tracks the project's current state, layout, and the consumer-facing API.

## What it does

For every registered program the indexer catches up from the program's `start_slot` to the current head, then stays live:

- **Accounts** — a per-account record of the latest state.
- **Transactions** — transactions touching a tracked program.
- **Events** — program log events (`Program data:` lines), decoded position-agnostically.

Data is stored raw and program-agnostic (bytes are base64 in responses, addresses/signatures are base58); consumers decode it themselves. `block_time` may be `null` immediately after ingest and is filled in by a background backfill pass.

## Consumer API

All read endpoints are public (no `x-security` header). Byte payloads (`data`) are base64; addresses, owners and signatures are base58. List endpoints share a `{ "data": [...], "total": N, "pages": N }` envelope.

### Accounts

- `GET /accounts/{address}` — latest state of one account.
- `GET /accounts?addresses=a,b,c` — batch fetch; the result is **positional**, with `null` where an address is not indexed.
- `GET /accounts/{address}/transactions` — indexed transactions that referenced the address, newest first (the `getSignaturesForAddress` replacement).

### Programs

- `GET /programs/{id}/accounts` — all accounts of a program.
  - `memcmp=<offset>:<base58>` — match raw bytes at a byte offset (offset is 0-indexed).
  - `dataSize=<n>` — match accounts whose state is exactly `n` bytes.
- `GET /programs/{id}/events` — program events, newest first.
  - `tx_signature=<base58>` — events from one transaction.
  - `discriminator=<hex>` — match the event's leading 8 bytes (hex, optional `0x` prefix).
- `GET /programs/{id}/transactions` — a program's transactions, newest first.
  - `success=true|false` — filter by outcome.

### Transactions

- `GET /transactions/{signature}` — a single transaction with its raw payload.
- `GET /transactions` — recent transactions across all programs, newest first.

### Slot filters & pagination

Read endpoints accept slot bounds where applicable: `since_slot` / `from_slot` (inclusive lower bound) and `to_slot` (inclusive upper bound). Pagination is opt-in: pass **both** `page` and `per_page` (per_page is clamped to 1..=100) to page; omit them to receive the whole result set in a single page.

## Streaming (SSE)

Live updates are delivered as [Server-Sent Events](https://developer.mozilla.org/docs/Web/API/Server-sent_events). Each stream is filtered server-side:

- `GET /sse/accounts/{address}` — changes to one account (`account` events).
- `GET /sse/programs/{id}/accounts` — every account change for a program (`account` events).
- `GET /sse/programs/{id}/events` — a program's log events (`event` events).

Event frames carry a JSON body with `slot` (and `write_version` for accounts, `signature` + `log_index` for events) plus the base64 `data`. A slow consumer that falls behind the fan-out buffer receives a `resync` event `{ "since_slot": <last seen slot>, "missed": <count> }` instead of silently losing changes.

### Reconnect pattern

To avoid gaps across reconnects or resyncs:

1. **Subscribe first.** Open the SSE stream before reading, so nothing published during backfill is missed.
2. **Backfill from the last slot you saw.** Call the matching read endpoint with `since_slot = <last processed slot>` to pull everything up to now.
3. **Handle `resync`.** When a `resync` frame arrives, treat its `since_slot` as the new backfill anchor: re-pull the gap via the read API, then keep consuming the stream.

The indexed history window starts at each program's `start_slot` — reads never return data older than that.

## Admin API

Admin endpoints require the `x-security` header (constant-time comparison) and manage indexing configuration:

- `GET /admin` — validate the admin passkey (used by the panel login gate).
- `GET /admin/stats` — indexer stats: counts, per-program progress (`last_processed_slot`), and table size estimates.
- `POST|GET|PATCH|DELETE /admin/rpcs[/{id}]` — manage RPC endpoints.
- `POST|GET|PATCH|DELETE /admin/programs[/{id}]` — manage indexed programs (register by `rpc_id` or inline `rpc_url`). An optional `account_filters` descriptor (`{ data_size, memcmp[], addresses[] }`) narrows ingestion to a subset of a program's accounts — only needed for chain-wide shared programs; omit it to track everything the program owns.
- `POST /admin/programs/{id}/activate` and `.../deactivate` — resume or pause indexing a program without deleting it.

A health check is served at `GET /` and an HTMX admin panel is served from `static/panel` at the root.

## Database schema

| Table | Purpose |
|---|---|
| `rpcs` | registered RPC endpoints (`http_url`, `ws_url`) |
| `programs` | programs to index, tied to an RPC (`address`, `name`, `start_slot`, `last_processed_slot`, `active`, `account_filters`) |
| `slots` | per-RPC slot dimension (`block_time`, filled once known) |
| `program_accounts` | per-program account registry holding each account's latest state |
| `transactions` | transactions touching a tracked program |
| `transaction_accounts` | per-transaction account references (with a `writable` flag) for address → transaction lookup |
| `program_events` | decoded program log events |

## Configuration

Read from the environment (see `.env.example`); parsed at startup.

| Var | Default | Purpose |
|---|---|---|
| `DATABASE_URL` | — | PostgreSQL connection string (required) |
| `ADMIN_PASSKEY` | — | secret for the `x-security` header (required) |
| `APP_PORT` | `3000` | HTTP listen port |
| `APP_ENV` | `development` | `development` enables TRACE logging; `production` uses INFO |
| `INDEXER_COMMITMENT` | `confirmed` | ingest commitment (`confirmed` or `finalized`) |
| `INDEXER_CATCHUP_CONCURRENCY` | `4` | max programs running bulk catch-up at once |
| `INDEXER_MAX_RETRIES` | `5` | rapid attempts (backoff + jitter) before a program indexer drops into cooldown; a healthy run resets the count |
| `INDEXER_RESTART_COOLDOWN_SECS` | `300` | base pause before a program's task restarts after the rapid-retry budget is exhausted; doubles on each consecutive rapid give-up and resets to the base after a healthy run |
| `INDEXER_RESTART_COOLDOWN_CAP_SECS` | `43200` | ceiling (12h) for the doubling cooldown, so a persistently broken program keeps retrying but only rarely; clamped up to the base if set lower |
| `INDEXER_RECONCILE_INTERVAL_SECS` | `300` | interval of the reconcile pass that respawns active programs whose task died out-of-band (e.g. a panic) |
| `INDEXER_BLOCKTIME_BACKFILL_SECS` | `60` | interval of the `block_time` backfill pass |
| `INDEXER_TX_SIGNATURE_BACKFILL_SECS` | `120` | interval of the `program_accounts.latest_tx_signature` backfill pass |
| `DB_MAX_CONNECTIONS` | `40` | pool ceiling |
| `DB_MIN_CONNECTIONS` | `5` | pool warm floor |
| `DB_ACQUIRE_TIMEOUT_SECS` | `8` | fail fast instead of hanging when the pool is exhausted |
| `DB_IDLE_TIMEOUT_SECS` | `300` | release idle connections when load drops |

Under `confirmed`, reorgs are rare and shallow: the latest state self-heals on the next change, while a row for a reorged slot may linger. Writes are idempotent `DO NOTHING` — there is no overwrite machinery in the MVP.

## Layout

```
src/
  main.rs             # binary entrypoint: env, db pool, logging, indexer, serve
  lib.rs              # app(): Router assembly (admin, sse, public reads, static panel)
  core/               # state, error, response, logging, sse event types
  middlewares/        # admin_guard (x-security header check)
  indexer/            # supervisor, per-program catch-up + live streams, persistence, rpc client
  routes/             # accounts, programs, transactions, sse, admin, payloads, responses
  db/entities/        # SeaORM entities
migration/            # SeaORM migrations
static/panel/         # HTMX admin panel
tests/unit/               # in-crate unit tests (pure logic, no db)
tests/integration_tests/  # httpc-test suite against a real server + test db
```

## Tech stack

- Rust 2024 edition
- Axum web framework (HTTP + SSE)
- SeaORM + PostgreSQL
- Solana RPC client
- JSON tracing subscriber + panic hook

## Running locally

1. Copy `.env.example` to `.env` and fill it in (at least `DATABASE_URL` and `ADMIN_PASSKEY`).

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

The suite has two tiers:

**Unit tests** (`tests/unit/`) cover the pure logic in isolation — the event log parser (CPI-stack attribution), URL validation, error→status mapping, pagination query coercion, and the `from_model` response conversions. They compile *into* the crate (a `#[cfg(test)]` `#[path]` module in `lib.rs`) so they can reach `pub(crate)`/internal items, and they touch **no database** (models are built in memory, nothing is mocked), so they need no `DATABASE_URL` and run instantly:

```bash
cargo test --lib
```

**Integration tests** (`tests/integration_tests/`) use `httpc-test` against a real server backed by its own `<database>_test` database, created and migrated automatically. They reset their tables before each case and must run single-threaded:

```bash
cargo test --test integration_tests -- --test-threads=1
```

To run **everything** in one go, pass the single-threaded flag (required by the integration tests) — the unit tests run fine under it too:

```bash
cargo test -- --test-threads=1
```

Set `TEST_DATABASE_URL` to override the test database; otherwise `DATABASE_URL` with a `_test` suffix on the database name is used.

## REST client

`request.http` contains example calls for every endpoint and works with the VS Code REST Client extension.

## Docker & CI locally

**Build and run the image** (serves the panel and API on port 3000). Migrations are *not* run by the container, so point it at an already-migrated database:

```bash
docker build -t svm-indexer .
docker run --rm -p 3000:3000 --env-file .env --network host svm-indexer
```

`--network host` lets the container reach a Postgres running on the host via a `localhost` `DATABASE_URL` (Linux). On macOS/Windows, drop `--network host` and use `host.docker.internal` as the DB host in `DATABASE_URL` instead.

**Run the GitHub CI locally** with [`act`](https://github.com/nektos/act) (needs Docker). It executes `.github/workflows/ci.yml` the way GitHub does, Postgres service container included:

```bash
act pull_request --pull=false
```

The workflow triggers `on: pull_request`, so pass that event. `--pull=false` reuses local images instead of force-pulling the service image on every run (pull it once with `docker pull postgres:16`). The Postgres service is published on host port **5433** (not 5432), so it coexists with a local `psql`, and `DATABASE_URL` points at `localhost:5433` — this holds on both GitHub's runner and `act`. The integration tests run single-threaded exactly as in CI. The first run also downloads the `act` runner image.
