use crate::core::{
    response::{ApiResponse, FinalResponse},
    state::AppState,
};
use axum::{Router, extract::State, http::StatusCode, routing::get};
use serde_json::{Value, json};

async fn get_root_handler(State(_state): State<AppState>) -> FinalResponse<Value> {
    Ok(json!({"status": "ok", "programs": []}).into())
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/health", get(get_root_handler))
}

pub async fn not_found_handler() -> FinalResponse<Value> {
    Ok(ApiResponse::new(
        StatusCode::NOT_FOUND,
        json!({"status": "ok", "message": "Route not found!"}),
    ))
}
