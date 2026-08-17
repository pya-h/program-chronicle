use std::collections::HashMap;
use std::sync::Arc;

use sea_orm::DatabaseConnection;
use serde::Serialize;
use tokio::sync::{Mutex, broadcast, mpsc};

use crate::core::sse::SseEvent;

/// Control signal the admin API sends to the indexer supervisor to start or stop a program's task.
#[derive(Debug, Clone, Copy)]
pub enum ProgramCommand {
    Start(i32),
    Stop(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProgramPhase {
    Backfilling,
    Syncing,
    Live,
    Restarting,
}

pub type ProgramPhases = Arc<Mutex<HashMap<i32, ProgramPhase>>>;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub admin_passkey: String, // To prevent rust to extract passkey from env everytime.
    pub supervisor_tx: Option<mpsc::Sender<ProgramCommand>>, // signals the supervisor to start/stop a program
    pub sse_tx: broadcast::Sender<SseEvent>, // fans live account/event changes out to SSE subscribers
    pub phases: Option<ProgramPhases>,
}

impl AppState {
    pub fn new(
        db: DatabaseConnection,
        admin_passkey: String,
        supervisor_tx: Option<mpsc::Sender<ProgramCommand>>,
        sse_tx: broadcast::Sender<SseEvent>,
        phases: Option<ProgramPhases>,
    ) -> Self {
        AppState {
            admin_passkey,
            db,
            supervisor_tx,
            sse_tx,
            phases,
        }
    }
}
