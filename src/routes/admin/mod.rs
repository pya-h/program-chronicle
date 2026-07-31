use axum::Router;

use crate::{core::state::AppState, middlewares::admin_guard::admin_guard};

pub mod rpcs;

pub fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        .merge(rpcs::routes())
        // add routes here
        .route_layer(axum::middleware::from_fn_with_state(state, admin_guard))
}
