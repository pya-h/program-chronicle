use crate::{
    core::{
        response::{ApiResponse, FinalResponse},
        state::AppState,
    },
    middlewares::admin_guard::admin_guard,
};
use axum::{Router, routing::get};

pub mod common;
mod errors;
pub mod programs;
mod rpcs;
mod stats;

async fn validate_admin_handler() -> FinalResponse<()> {
    Ok(ApiResponse::ok(())) // Panel could call this, if it only wants to check passkey is correct
}

pub fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        .nest("/rpcs", rpcs::routes())
        .nest("/programs", programs::routes())
        .nest("/stats", stats::routes())
        .nest("/errors", errors::routes())
        .route("/", get(validate_admin_handler))
        // add routes here
        .route_layer(axum::middleware::from_fn_with_state(state, admin_guard))
}
