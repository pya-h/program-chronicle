use sea_orm::{ConnectOptions, Database};
use solana_sdk::commitment_config::CommitmentConfig;
use std::{env, net::SocketAddr, sync::Arc, time::Duration};
use svm_indexer::core::sse::{SSE_CHANNEL_CAPACITY, SseEvent};
use svm_indexer::core::state::ProgramCommand;
use svm_indexer::indexer::Indexer;
use tokio::sync::{broadcast, mpsc};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    if let Err(err) = svm_indexer::core::logging::setup() {
        panic!("Logger setup failed: {}", err);
    }

    let env_u64 = |key: &str, default: u64| -> u64 {
        let raw = env::var(key).unwrap_or_default().trim().to_string();
        if raw.is_empty() {
            default
        } else {
            raw.parse()
                .unwrap_or_else(|_| panic!("{key} must be a positive integer!"))
        }
    };

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL not set!");
    let mut db_options = ConnectOptions::new(database_url);
    db_options
        .max_connections(env_u64("DB_MAX_CONNECTIONS", 40) as u32)
        .min_connections(env_u64("DB_MIN_CONNECTIONS", 5) as u32)
        .acquire_timeout(Duration::from_secs(env_u64("DB_ACQUIRE_TIMEOUT_SECS", 8)))
        .idle_timeout(Duration::from_secs(env_u64("DB_IDLE_TIMEOUT_SECS", 300)));
    let database_connection = match Database::connect(db_options).await {
        Ok(conn) => conn,
        Err(err) => {
            panic!("Database connection failed: {err}");
        }
    };

    let (supervisor_tx, supervisor_rx) = mpsc::channel::<ProgramCommand>(32);
    let (sse_tx, _) = broadcast::channel::<SseEvent>(SSE_CHANNEL_CAPACITY);
    let restart_cooldown_secs = env_u64("INDEXER_RESTART_COOLDOWN_SECS", 300);
    let restart_cooldown_cap_secs =
        env_u64("INDEXER_RESTART_COOLDOWN_CAP_SECS", 43_200).max(restart_cooldown_secs);
    let indexer = Arc::new(Indexer::new(
        database_connection.clone(),
        match env::var("INDEXER_COMMITMENT")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "finalized" => CommitmentConfig::finalized(),
            _ => CommitmentConfig::confirmed(),
        },
        sse_tx.clone(),
        env_u64("INDEXER_CATCHUP_CONCURRENCY", 4).max(1) as usize, // 0 permits would deadlock every catch-up
        env_u64("INDEXER_MAX_RETRIES", 5) as u32, // RetryPolicy clamps to at least one attempt
        Duration::from_secs(restart_cooldown_secs),
        Duration::from_secs(restart_cooldown_cap_secs),
    ));

    let backfill_interval =
        Duration::from_secs(env_u64("INDEXER_BLOCKTIME_BACKFILL_SECS", 60).max(1)); // interval panics on a zero period
    tokio::spawn(indexer.clone().run_block_time_backfill(backfill_interval));
    let tx_signature_backfill_interval =
        Duration::from_secs(env_u64("INDEXER_TX_SIGNATURE_BACKFILL_SECS", 120).max(1));
    tokio::spawn(
        indexer
            .clone()
            .run_tx_signature_backfill(tx_signature_backfill_interval),
    );
    let reconcile_interval =
        Duration::from_secs(env_u64("INDEXER_RECONCILE_INTERVAL_SECS", 300).max(1));
    tokio::spawn(indexer.clone().run_reconciler(reconcile_interval));
    let commitment_worker_interval =
        Duration::from_secs(env_u64("INDEXER_COMMITMENT_WORKER_SECS", 60).max(1));
    tokio::spawn(
        indexer
            .clone()
            .run_commitment_worker(commitment_worker_interval),
    );
    let phases = indexer.phases();
    tokio::spawn(async move {
        if let Err(err) = indexer.start(supervisor_rx).await {
            tracing::error!(error = %err, "Indexer supervisor failed to start!");
        }
    });

    let admin_passkey = env::var("ADMIN_PASSKEY").expect("ADMIN_PASSKEY not set!");
    assert!(
        !admin_passkey.trim().is_empty(),
        "ADMIN_PASSKEY must not be empty!"
    );
    let router = svm_indexer::app(
        database_connection,
        admin_passkey,
        Some(supervisor_tx),
        sse_tx,
        Some(phases),
    );

    let app_port = {
        let port = env::var("APP_PORT").unwrap_or_default().trim().to_string();
        if !port.is_empty() {
            port.parse().expect("APP_PORT must be a positive integer!")
        } else {
            3000
        }
    };
    let addr = SocketAddr::from(([0, 0, 0, 0], app_port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|_| panic!("Failed listening on the port: {}", app_port));
    tracing::info!("Listening on {}...", addr);
    axum::serve(listener, router)
        .await
        .expect("Failed serving and setting up routes with the tcp listener!");
}
