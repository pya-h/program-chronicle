use std::env;

use sea_orm::DatabaseConnection;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub admin_passkey: String, // To prevent rust to extract passkey from env everytime.
}

impl AppState {
    pub fn new(db: DatabaseConnection) -> Self {
        let admin_passkey = env::var("ADMIN_PASSKEY").expect("ADMIN_PASSKEY not set!");
        AppState {
            admin_passkey,
            db,
        }
    }
}
