use std::sync::OnceLock;
use std::time::Duration;

use httpc_test::Client;
use migration::MigratorTrait;
use sea_orm::{ConnectionTrait, ConnectOptions, Database};
use tokio::runtime::Runtime;

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

fn setup_server() -> ServerInfo {
    dotenvy::dotenv().ok();
    let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for tests");
    let admin_passkey = std::env::var("ADMIN_PASSKEY").unwrap_or_else(|_| "test".to_string());

    let (tx, rx) = std::sync::mpsc::channel();
    let passkey_for_thread = admin_passkey.clone();

    std::thread::spawn(move || {
        let rt = Runtime::new().expect("failed to create test server runtime");
        let port = rt.block_on(async {
            let mut opts = ConnectOptions::new(&db_url);
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
    let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for tests");
    let db = Database::connect(db_url)
        .await
        .expect("failed to connect to test database for reset");
    db.execute_unprepared("DELETE FROM programs; DELETE FROM rpcs;")
        .await
        .expect("failed to reset test tables");
}
