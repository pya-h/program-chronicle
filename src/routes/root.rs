use crate::core::{response::*, state::AppState};
use axum::{Json, Router, extract::State, routing::get};
use serde_json::{Value, json};

async fn get_root_handler(State(_state): State<AppState>) -> FinalResponse<Value> {
    Ok(Json(json!({"status": "ok", "programs": []})))
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/", get(get_root_handler))
}

pub async fn not_found_handler() -> FinalResponse<Value> {
    Ok(Json(json!({"status": "ok", "message": "Route not found!"})))
}
