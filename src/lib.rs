pub mod core;
pub mod db;
pub mod indexer;
pub mod middlewares;
pub mod routes;

use axum::{Router, routing::get};
use sea_orm::DatabaseConnection;
use tower_http::services::ServeDir;

use crate::core::state::AppState;

pub fn app(db: DatabaseConnection, admin_passkey: String) -> Router {
    let state = AppState::new_with_passkey(db, admin_passkey);
    Router::new()
        .nest("/admin", routes::admin::routes(state.clone()))
        .merge(routes::root::routes())
        .with_state(state)
        .fallback_service(
            ServeDir::new("static/panel")
                .not_found_service(get(routes::root::not_found_handler)),
        )
}
