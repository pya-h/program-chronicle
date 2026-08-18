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

#[derive(Debug, Clone, Copy)]
pub struct ProgramRuntime {
    pub phase: ProgramPhase,
    pub head_slot: Option<i64>,
}

impl ProgramRuntime {
    pub fn new(phase: ProgramPhase) -> Self {
        Self {
            phase,
            head_slot: None,
        }
    }
}

pub type ProgramRuntimes = Arc<Mutex<HashMap<i32, ProgramRuntime>>>;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub admin_passkey: String, // To prevent rust to extract passkey from env everytime.
    pub supervisor_tx: Option<mpsc::Sender<ProgramCommand>>, // signals the supervisor to start/stop a program
    pub sse_tx: broadcast::Sender<SseEvent>, // fans live account/event changes out to SSE subscribers
    pub runtimes: Option<ProgramRuntimes>,
}

impl AppState {
    pub fn new(
        db: DatabaseConnection,
        admin_passkey: String,
        supervisor_tx: Option<mpsc::Sender<ProgramCommand>>,
        sse_tx: broadcast::Sender<SseEvent>,
        runtimes: Option<ProgramRuntimes>,
    ) -> Self {
        AppState {
            admin_passkey,
            db,
            supervisor_tx,
            sse_tx,
            runtimes,
        }
    }
}
