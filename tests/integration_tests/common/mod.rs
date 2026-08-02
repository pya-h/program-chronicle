use std::sync::OnceLock;
use std::time::Duration;

use chrono::{DateTime, Utc};
use httpc_test::Client;
use migration::MigratorTrait;
use sea_orm::{ConnectOptions, ConnectionTrait, Database};
use serde_json::Value;
use tokio::runtime::Runtime;
use url::Url;

static SERVER: OnceLock<ServerInfo> = OnceLock::new();

pub struct ServerInfo {
    pub base_url: String,
    pub admin_passkey: String,
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

/// Ensure the test database exists and is reachable, creating it via the
/// `postgres` maintenance database if necessary.
async fn ensure_test_database(test_url: &str) {
    // Fast path: the database already exists and is reachable.
    // We must execute a query because Database::connect creates a lazy pool.
    if let Ok(db) = Database::connect(test_url).await {
        if db.execute_unprepared("SELECT 1").await.is_ok() {
            return;
        }
    }

    // Slow path: create it via the maintenance database if it doesn't exist.
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
        // Ignore "database already exists" (SQLSTATE 42P04) so concurrent or
        // re-runs don't panic.
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
        let port = rt.block_on(async {
            ensure_test_database(&db_url_for_thread).await;

            let mut opts = ConnectOptions::new(db_url_for_thread);
            opts.max_connections(10);
            let db = Database::connect(opts)
                .await
                .expect("failed to connect app database");

            migration::Migrator::refresh(&db)
                .await
                .expect("failed to refresh test database schema");

            let app = svm_indexer::app(db, passkey_for_thread.clone());
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("failed to bind test server");
            let port = listener.local_addr().unwrap().port();

            tokio::spawn(async move {
                axum::serve(listener, app).await.unwrap();
            });
            tokio::time::sleep(Duration::from_millis(100)).await;

            port
        });
        tx.send(port).expect("failed to send server port");

        // Keep the server runtime alive for the whole test process.
        rt.block_on(std::future::pending::<()>());
    });

    let port = rx.recv().expect("failed to receive server port");

    ServerInfo {
        base_url: format!("http://127.0.0.1:{}", port),
        admin_passkey,
    }
}

pub fn server() -> &'static ServerInfo {
    SERVER.get_or_init(setup_server)
}

pub async fn reset_db() {
    dotenvy::dotenv().ok();
    // Force the server (and therefore the test database + migrations) to be
    // initialized before we try to delete anything. Without this, the first
    // test could run reset_db() against a database that has not been created yet.
    let _ = server();

    let db_url = test_database_url();
    let db = Database::connect(db_url)
        .await
        .expect("failed to connect to test database for reset");
    db.execute_unprepared("DELETE FROM programs; DELETE FROM rpcs;")
        .await
        .expect("failed to reset test tables");
}

pub fn parse_datetime(value: &Value) -> DateTime<Utc> {
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
