use super::payloads::admin::*;
use crate::core::{response::FinalResponse, response::SvmIndexerError, state::AppState};
use axum::{Json, extract::State};
use serde_json::{Value, json};
use url::Url;

async fn add_rpc(
    state: &AppState,
    http_url: String,
    ws_url: Option<String>,
) -> Result<(), SvmIndexerError> {
    if http_url.is_empty() {
        return Err(SvmIndexerError::InvalidURL(String::from(
            "RPC's http_url not provided!",
        )));
    }
    let http_url = if ["http://", "https://"]
        .iter()
        .any(|prefix| http_url.starts_with(prefix))
    {
        http_url
    } else {
        format!("https://{}", http_url).to_string()
    };

    if let Err(err) = Url::parse(&http_url) {
        return Err(SvmIndexerError::InvalidURL(format!(
            "Invalid http RPC provided: {}",
            err
        )));
    }
    let ws_url = ws_url.map_or(http_url.replace("http", "ws"), |input| {
        if ["ws", "wss"].iter().any(|prefix| input.starts_with(prefix)) {
            input
        } else {
            format!("wss://{}", input)
        }
    });
    if let Err(err) = Url::parse(&ws_url) {
        return Err(SvmIndexerError::InvalidURL(format!(
            "Invalid web socket RPC provided! {}",
            err
        )));
    }
    Ok(())
}

async fn add_rpc_handler(
    State(state): State<AppState>,
    Json(payload): Json<AddRpcPayload>,
) -> FinalResponse<Value> {
    add_rpc(
        &state,
        payload.http_url.trim().to_string(),
        payload.ws_url.map(|u| u.trim().to_string()),
    )
    .await?;
    Ok(json!({}).into())
}
