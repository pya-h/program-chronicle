use std::env;

use sea_orm::DatabaseConnection;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub admin_passkey: String, // To prevent rust to extract passkey from env everytime.
}

impl AppState {
    pub fn new_with_passkey(db: DatabaseConnection, admin_passkey: String) -> Self {
        AppState {
            admin_passkey,
            db,
        }
    }

    pub fn new(db: DatabaseConnection) -> Self {
        let admin_passkey = env::var("ADMIN_PASSKEY").expect("ADMIN_PASSKEY not set!");
        Self::new_with_passkey(db, admin_passkey)
    }
}
