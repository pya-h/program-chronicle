use sea_orm::DatabaseConnection;
use tokio::sync::{broadcast, mpsc};

use crate::core::sse::SseEvent;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub admin_passkey: String, // To prevent rust to extract passkey from env everytime.
    pub new_program_tx: Option<mpsc::Sender<i32>>, // notifies the supervisor to index a just-added program
    pub sse_tx: broadcast::Sender<SseEvent>, // fans live account/event changes out to SSE subscribers
}

impl AppState {
    pub fn new(
        db: DatabaseConnection,
        admin_passkey: String,
        new_program_tx: Option<mpsc::Sender<i32>>,
        sse_tx: broadcast::Sender<SseEvent>,
    ) -> Self {
        AppState {
            admin_passkey,
            db,
            new_program_tx,
            sse_tx,
        }
    }
}
