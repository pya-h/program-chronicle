use super::payloads::admin::*;
use crate::core::{response::FinalResponse, state::AppState};
use axum::{Json, extract::State};
use serde_json::{Value, json};

async fn add_rpc(
    State(state): State<AppState>,
    Json(payload): Json<AddRpcPayload>,
) -> FinalResponse<Value> {

    Ok(json!({}).into())
}
