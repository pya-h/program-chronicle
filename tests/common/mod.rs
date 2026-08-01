use httpc_test::Client;
use sea_orm::{ConnectionTrait, Database, DatabaseConnection};
use tokio::sync::{Mutex, OnceCell};

static TEST_STATE: OnceCell<Mutex<TestState>> = OnceCell::const_new();

pub struct TestState {
    pub db: DatabaseConnection,
    pub authed_client: Client,
    pub unauthed_client: Client,
}

pub async fn setup() -> &'static Mutex<TestState> {
    TEST_STATE
        .get_or_init(|| async {
            let db_url =
                std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for tests");
            let admin_passkey =
                std::env::var("ADMIN_PASSKEY").unwrap_or_else(|_| "test".to_string());

            let db = Database::connect(&db_url)
                .await
                .expect("failed to connect to test database");
            migration::Migrator::refresh(&db)
                .await
                .expect("failed to refresh test database schema");

            let app = svm_indexer::app(db.clone(), admin_passkey.clone());
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("failed to bind test server");
            let port = listener.local_addr().unwrap().port();

            tokio::spawn(async move {
                axum::serve(listener, app).await.unwrap();
            });
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            let base_url = format!("http://127.0.0.1:{}", port);

            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert(
                "x-security",
                reqwest::header::HeaderValue::from_str(&admin_passkey).unwrap(),
            );
            let authed_client = httpc_test::new_client_with_reqwest(
                base_url.clone(),
                reqwest::Client::builder().default_headers(headers),
            )
            .expect("failed to create authed test client");

            let unauthed_client =
                httpc_test::new_client(base_url).expect("failed to create unauthed test client");

            Mutex::new(TestState {
                db,
                authed_client,
                unauthed_client,
            })
        })
        .await
}

pub async fn reset_db(state: &TestState) {
    state
        .db
        .execute_unprepared("TRUNCATE TABLE programs, rpcs RESTART IDENTITY CASCADE")
        .await
        .expect("failed to reset test tables");
}
