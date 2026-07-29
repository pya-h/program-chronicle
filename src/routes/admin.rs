use super::payloads::admin::*;
use crate::middlewares::admin_guard::admin_guard;
use crate::{
    core::{
        error::SvmIndexerError,
        response::{ApiResponse, FinalResponse},
        state::AppState,
    },
    db::entities::rpcs,
};
use axum::{Json, Router, extract::State, routing::post};
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use serde_json::{Value, json};
use tracing::error;
use url::{ParseError, Url};

async fn add_rpc(
    state: &AppState,
    http_url: String,
    ws_url: Option<String>,
) -> Result<rpcs::Model, SvmIndexerError> {
    if http_url.is_empty() {
        return Err(SvmIndexerError::InvalidURL(String::from(
            "RPC's http_url not provided!",
        )));
    }
    let http_url = if http_url.contains("://") {
        http_url
    } else {
        format!("https://{}", http_url).to_string()
    };

    if let Err(err) = Url::parse(&http_url).and_then(|h| {
        if matches!(h.scheme(), "http" | "https") {
            Ok(())
        } else {
            Err(ParseError::InvalidIpv4Address)
        }
    }) {
        return Err(SvmIndexerError::InvalidURL(format!(
            "Invalid http RPC provided: {}",
            err
        )));
    }
    let ws_url = ws_url.map_or(http_url.replace("http", "ws"), |input| {
        if input.contains("://") {
            input
        } else {
            format!("wss://{}", input)
        }
    });
    if let Err(err) = Url::parse(&ws_url).and_then(|w| {
        if matches!(w.scheme(), "ws" | "wss") {
            Ok(())
        } else {
            Err(ParseError::InvalidIpv4Address)
        }
    }) {
        return Err(SvmIndexerError::InvalidURL(format!(
            "Invalid web socket RPC provided! {}",
            err
        )));
    }

    let new_rpc = rpcs::ActiveModel {
        http_url: Set(http_url),
        ws_url: Set(ws_url),
        ..Default::default()
    };

    Ok(new_rpc.insert(&state.db).await.map_err(|err| {
        SvmIndexerError::InternalError(format!("Failed adding the new RPC: {}", err))
    })?)
}

async fn add_rpc_handler(
    State(state): State<AppState>,
    Json(payload): Json<AddRpcPayload>,
) -> FinalResponse<Value> {
    let new_rpc = add_rpc(
        &state,
        payload.http_url.trim().to_string(),
        payload.ws_url.map(|u| u.trim().to_string()),
    )
    .await?;

    Ok(ApiResponse::created(json!({ "rpc": new_rpc })))
}

pub fn routes(state: AppState) -> Router<AppState> {
    Router::new().route(
        "/admin/rpcs",
        post(add_rpc_handler).route_layer(axum::middleware::from_fn_with_state(state, admin_guard)),
    )
}
