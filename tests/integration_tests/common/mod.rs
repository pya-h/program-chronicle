use std::sync::OnceLock;
use std::time::Duration;
use httpc_test::Client;
use tokio::sync::{Mutex, MutexGuard};
use migration::MigratorTrait;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ConnectOptions, ConnectionTrait, Database,
    DatabaseConnection,
};
use serde_json::Value;
use solana_sdk::{pubkey::Pubkey, signature::Signature};
use tokio::runtime::Runtime;
use tokio::sync::broadcast;
use url::Url;

use svm_indexer::core::sse::SseEvent;
use svm_indexer::db::entities::{program_events, programs, rpcs, transactions};
use svm_indexer::indexer::persist::{ensure_slot, upsert_account_snapshot};
use svm_indexer::indexer::rpc::AccountSnapshot;

static SERVER: OnceLock<ServerInfo> = OnceLock::new();

pub struct ServerInfo {
    pub base_url: String,
    pub admin_passkey: String,
    pub sse_tx: broadcast::Sender<SseEvent>,
}

impl ServerInfo {
    pub fn authed_client(&self) -> Client {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "x-security",
            reqwest::header::HeaderValue::from_str(&self.admin_passkey).unwrap(),
        );
        httpc_test::new_client_with_reqwest(
            self.base_url.clone(),
            reqwest::Client::builder().default_headers(headers),
        )
        .expect("failed to create authed test client")
    }

    pub fn unauthed_client(&self) -> Client {
        httpc_test::new_client(self.base_url.clone())
            .expect("failed to create unauthed test client")
    }
}

fn database_name_from_url(url: &Url) -> String {
    url.path_segments()
        .and_then(|mut s| s.next_back())
        .filter(|s| !s.is_empty())
        .expect("URL must contain a database name")
        .to_string()
}

fn test_database_url() -> String {
    if let Ok(test_url) = std::env::var("TEST_DATABASE_URL") {
        return test_url;
    }

    let main_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set for tests (or set TEST_DATABASE_URL instead)");

    let mut url = Url::parse(&main_url)
        .unwrap_or_else(|_| panic!("DATABASE_URL is not a valid URL: {main_url}"));

    // Connection string may contain params; we extract the *decoded* db name,
    // append _test, and preserve the rest of the URL (query params, credentials, etc.).
    let db_name = database_name_from_url(&url);

    url.set_path(&format!("{db_name}_test"));
    url.to_string()
}

async fn ensure_test_database(test_url: &str) {
    if let Ok(db) = Database::connect(test_url).await {
        if db.execute_unprepared("SELECT 1").await.is_ok() {
            return;
        }
    }

    let url =
        Url::parse(test_url).expect("test database URL should be valid because we just built it");

    let mut maintenance_url = url.clone();
    maintenance_url.set_path("postgres");

    let maintenance_db = Database::connect(maintenance_url.to_string())
        .await
        .expect("failed to connect to maintenance database to create test database");

    let db_name = database_name_from_url(&url);

    if let Err(err) = maintenance_db
        .execute_unprepared(&format!(
            "CREATE DATABASE \"{}\"",
            db_name.replace('"', "\"\"")
        ))
        .await
    {
        let err_str = err.to_string();
        if !err_str.contains("42P04") && !err_str.contains("already exists") {
            panic!("failed to create test database: {err}");
        }
    }

    // Verify the new database is reachable.
    let db = Database::connect(test_url)
        .await
        .expect("failed to connect to freshly created test database");
    db.execute_unprepared("SELECT 1")
        .await
        .expect("freshly created test database is not reachable");
}

fn setup_server() -> ServerInfo {
    dotenvy::dotenv().ok();
    let db_url = test_database_url();
    let admin_passkey = std::env::var("ADMIN_PASSKEY").unwrap_or_else(|_| "test".to_string());

    let (tx, rx) = std::sync::mpsc::channel();
    let passkey_for_thread = admin_passkey.clone();
    let db_url_for_thread = db_url.clone();

    std::thread::spawn(move || {
        let rt = Runtime::new().expect("failed to create test server runtime");
        let (port, sse_tx) = rt.block_on(async {
            ensure_test_database(&db_url_for_thread).await;

            let mut opts = ConnectOptions::new(db_url_for_thread);
            opts.max_connections(10);
            let db = Database::connect(opts)
                .await
                .expect("failed to connect app database");

            migration::Migrator::refresh(&db)
                .await
                .expect("failed to refresh test database schema");

            let (sse_tx, _) = broadcast::channel::<SseEvent>(64);
            let app = svm_indexer::app(db, passkey_for_thread.clone(), None, sse_tx.clone());
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("failed to bind test server");
            let port = listener.local_addr().unwrap().port();

            tokio::spawn(async move {
                axum::serve(listener, app).await.unwrap();
            });
            tokio::time::sleep(Duration::from_millis(100)).await;

            (port, sse_tx)
        });
        tx.send((port, sse_tx)).expect("failed to send server handle");

        // Keep the server runtime alive for the whole test process.
        rt.block_on(std::future::pending::<()>());
    });

    let (port, sse_tx) = rx.recv().expect("failed to receive server handle");

    ServerInfo {
        base_url: format!("http://127.0.0.1:{}", port),
        admin_passkey,
        sse_tx,
    }
}

pub fn server() -> &'static ServerInfo {
    SERVER.get_or_init(setup_server)
}

/// A fresh connection to the test database, sharing the same schema the running server uses.
pub async fn test_db() -> DatabaseConnection {
    dotenvy::dotenv().ok();
    let _ = server();
    Database::connect(test_database_url())
        .await
        .expect("failed to connect to test database")
}

/// Serializes tests against the single shared database: every test starts with `reset_db().await`,
/// which wipes global state, so two tests running at once would clobber each other's rows and
/// counts. Each test binds the returned guard (`let _guard = reset_db().await;`) for its whole
/// body; `tokio::sync::Mutex` is used because it does not poison when a test panics.
static DB_GUARD: Mutex<()> = Mutex::const_new(());

pub async fn reset_db() -> MutexGuard<'static, ()> {
    let guard = DB_GUARD.lock().await;
    let db = test_db().await;
    // child rows first so the Restrict foreign keys don't block the truncation
    db.execute_unprepared(
        "DELETE FROM account_states; DELETE FROM program_events; DELETE FROM transactions; \
         DELETE FROM program_accounts; DELETE FROM slots; DELETE FROM programs; DELETE FROM rpcs;",
    )
    .await
    .expect("failed to reset test tables");
    guard
}

/// Deterministic 32-byte address from a seed, so tests can round-trip the base58 string.
pub fn pubkey(seed: u8) -> Pubkey {
    Pubkey::new_from_array([seed; 32])
}

/// Deterministic 64-byte signature from a seed.
pub fn signature(seed: u8) -> Signature {
    Signature::from([seed; 64])
}

pub async fn seed_rpc(db: &DatabaseConnection, http_url: &str) -> i32 {
    let now = chrono::Utc::now().naive_utc();
    rpcs::ActiveModel {
        http_url: Set(http_url.to_owned()),
        ws_url: Set(format!("wss://{}", http_url.trim_start_matches("https://"))),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("failed to seed rpc")
    .id
}

pub async fn seed_program(db: &DatabaseConnection, rpc_id: i32, address: &Pubkey) -> i32 {
    let now = chrono::Utc::now().naive_utc();
    programs::ActiveModel {
        rpc_id: Set(rpc_id),
        address: Set(address.to_bytes().to_vec()),
        name: Set(Some("seed-program".to_owned())),
        start_slot: Set(Some(0)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("failed to seed program")
    .id
}

/// Persists an account snapshot through the real indexer path, so seeded reads exercise the same
/// cache/history rows production writes. Returns nothing; the caller reads back via the API.
#[allow(clippy::too_many_arguments)]
pub async fn seed_account(
    db: &DatabaseConnection,
    program_id: i32,
    rpc_id: i32,
    address: &Pubkey,
    owner: &Pubkey,
    data: &[u8],
    lamports: u64,
    slot: u64,
    write_version: u64,
    block_time: Option<i64>,
) {
    let (sse_tx, _) = broadcast::channel::<SseEvent>(16);
    let snapshot = AccountSnapshot {
        address: *address,
        owner: *owner,
        lamports,
        data: data.to_vec(),
        rent_epoch: 0,
        executable: false,
        slot,
        write_version: Some(write_version),
    };
    upsert_account_snapshot(db, &sse_tx, program_id, rpc_id, &snapshot, None, block_time)
        .await
        .expect("failed to seed account snapshot");
}

pub async fn seed_transaction(
    db: &DatabaseConnection,
    program_id: Option<i32>,
    rpc_id: i32,
    signature: &Signature,
    slot: i64,
    success: bool,
    block_time: Option<i64>,
) {
    let slot_id = ensure_slot(db, rpc_id, slot, block_time)
        .await
        .expect("failed to ensure slot for transaction");
    let now = chrono::Utc::now().naive_utc();
    transactions::ActiveModel {
        signature: Set(signature.as_ref().to_vec()),
        program_id: Set(program_id),
        slot: Set(slot),
        slot_id: Set(slot_id),
        success: Set(success),
        error: Set(None),
        fee: Set(Some(5000)),
        fee_payer: Set(None),
        raw: Set(serde_json::json!({"seed": true})),
        created_at: Set(now),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("failed to seed transaction");
}

pub async fn seed_event(
    db: &DatabaseConnection,
    program_id: i32,
    rpc_id: i32,
    signature: &Signature,
    log_index: i32,
    slot: i64,
    data: &[u8],
    block_time: Option<i64>,
) {
    let slot_id = ensure_slot(db, rpc_id, slot, block_time)
        .await
        .expect("failed to ensure slot for event");
    let now = chrono::Utc::now().naive_utc();
    program_events::ActiveModel {
        program_id: Set(program_id),
        program_account_id: Set(None),
        tx_signature: Set(signature.as_ref().to_vec()),
        slot: Set(slot),
        slot_id: Set(slot_id),
        log_index: Set(log_index),
        data: Set(data.to_vec()),
        created_at: Set(now),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("failed to seed event");
}

pub fn parse_datetime(value: &Value) -> chrono::NaiveDateTime {
    value
        .as_str()
        .expect("expected a datetime string")
        .parse()
        .expect("expected a valid ISO 8601 datetime")
}

pub fn assert_datetimes_equal(left: &Value, right: &Value) {
    assert_eq!(
        parse_datetime(left),
        parse_datetime(right),
        "expected datetimes to be equal"
    );
}
