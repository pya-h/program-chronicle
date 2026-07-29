use crate::core::{response::FinalResponse, state::AppState};
use axum::{Router, extract::State, routing::get};
use serde_json::{Value, json};

async fn get_root_handler(State(_state): State<AppState>) -> FinalResponse<Value> {
    Ok(json!({"status": "ok", "programs": []}).into())
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/", get(get_root_handler))
}

pub async fn not_found_handler() -> FinalResponse<Value> {
    Ok(json!({"status": "ok", "message": "Route not found!"}).into())
}
