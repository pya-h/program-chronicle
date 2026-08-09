use std::env;

use sea_orm::DatabaseConnection;
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub admin_passkey: String, // To prevent rust to extract passkey from env everytime.
    pub new_program_tx: Option<mpsc::Sender<i32>>, // notifies the supervisor to index a just-added program
}

impl AppState {
    pub fn new_with_passkey(
        db: DatabaseConnection,
        admin_passkey: String,
        new_program_tx: Option<mpsc::Sender<i32>>,
    ) -> Self {
        AppState {
            admin_passkey,
            db,
            new_program_tx,
        }
    }

    pub fn new(db: DatabaseConnection) -> Self {
        let admin_passkey = env::var("ADMIN_PASSKEY").expect("ADMIN_PASSKEY not set!");
        Self::new_with_passkey(db, admin_passkey, None)
    }
}
