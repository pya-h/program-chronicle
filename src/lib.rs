pub mod core;
pub mod db;
pub mod indexer;
pub mod middlewares;
pub mod routes;

#[cfg(test)]
#[path = "../tests/unit/mod.rs"]
mod unit;

use axum::{Router, response::Redirect, routing::get};
use sea_orm::DatabaseConnection;
use tokio::sync::{broadcast, mpsc};
use tower_http::services::{ServeDir, ServeFile};

use crate::core::sse::SseEvent;
use crate::core::state::{AppState, ProgramCommand, ProgramPhases};

pub fn app(
    db: DatabaseConnection,
    admin_passkey: String,
    supervisor_tx: Option<mpsc::Sender<ProgramCommand>>,
    sse_tx: broadcast::Sender<SseEvent>,
    phases: Option<ProgramPhases>,
) -> Router {
    let state = AppState::new(db, admin_passkey, supervisor_tx, sse_tx, phases);
    Router::new()
        .nest("/admin", routes::admin::routes(state.clone()))
        .nest("/sse", routes::sse::routes())
        .nest("/accounts", routes::accounts::routes())
        .nest("/programs", routes::programs::routes())
        .nest("/transactions", routes::transactions::routes())
        .merge(routes::root::routes())
        .route("/panel", get(|| async { Redirect::permanent("/panel/") }))
        .nest_service(
            "/panel/",
            ServeDir::new("static/panel")
                .not_found_service(ServeFile::new("static/panel/404.html")),
        )
        .with_state(state)
        .fallback(routes::root::not_found_handler)
}
