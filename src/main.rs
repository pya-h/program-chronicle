use std::{env, net::SocketAddr, sync::Arc, time::Duration};
use sea_orm::Database;
use solana_sdk::commitment_config::CommitmentConfig;
use svm_indexer::core::sse::{SSE_CHANNEL_CAPACITY, SseEvent};
use svm_indexer::indexer::Indexer;
use tokio::sync::{broadcast, mpsc};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    if let Err(err) = svm_indexer::core::logging::setup() {
        panic!("Logger setup failed: {}", err);
    }

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL not set!");
    let database_connection = match Database::connect(&database_url).await {
        Ok(conn) => conn,
        Err(err) => {
            panic!(
                "Database connection failed: {}",
                err.sql_err().expect("Unknown error happened!")
            );
        }
    };

    let (new_program_tx, new_program_rx) = mpsc::channel::<i32>(32);
    let (sse_tx, _) = broadcast::channel::<SseEvent>(SSE_CHANNEL_CAPACITY);
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
    ));

    let backfill_interval = Duration::from_secs({
        let secs = env::var("INDEXER_BLOCKTIME_BACKFILL_SECS")
            .unwrap_or_default()
            .trim()
            .to_string();
        if !secs.is_empty() {
            secs.parse()
                .expect("INDEXER_BLOCKTIME_BACKFILL_SECS must be a positive integer!")
        } else {
            60
        }
    });
    tokio::spawn(indexer.clone().run_block_time_backfill(backfill_interval));
    tokio::spawn(async move {
        if let Err(err) = indexer.start(new_program_rx).await {
            tracing::error!(error = %err, "Indexer supervisor failed to start!");
        }
    });

    let admin_passkey = env::var("ADMIN_PASSKEY").expect("ADMIN_PASSKEY not set!");
    let router = svm_indexer::app(database_connection, admin_passkey, Some(new_program_tx), sse_tx);

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
        .expect(&format!("Failed listening on the port: {}", app_port));
    tracing::info!("Listening on {}...", addr);
    axum::serve(listener, router)
        .await
        .expect("Failed serving and setting up routes with the tcp listener!");
}
