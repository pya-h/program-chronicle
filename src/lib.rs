pub mod core;
pub mod db;
pub mod middlewares;
pub mod routes;

use axum::Router;
use sea_orm::DatabaseConnection;

use crate::core::state::AppState;

pub fn app(db: DatabaseConnection, admin_passkey: String) -> Router {
    let state = AppState::new_with_passkey(db, admin_passkey);
    Router::new()
        .nest("/admin", routes::admin::routes(state.clone()))
        .merge(routes::root::routes())
        .with_state(state)
        .fallback(routes::root::not_found_handler)
}
