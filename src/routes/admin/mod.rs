use axum::Router;

use crate::{core::state::AppState, middlewares::admin_guard::admin_guard};

mod rpcs;
pub mod common;
pub mod programs;

pub fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        .nest("/rpcs", rpcs::routes())
        .nest("/programs", programs::routes())
        // add routes here
        .route_layer(axum::middleware::from_fn_with_state(state, admin_guard))
}
