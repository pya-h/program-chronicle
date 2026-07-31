use super::payloads::admin::*;
use super::payloads::common::*;
use super::responses::admin::*;
use crate::db::entities::programs::{self, Model as ProgramModel};
use crate::db::entities::rpcs::Model as RpcModel;
use crate::middlewares::admin_guard::admin_guard;
use crate::{
    core::{
        error::SvmIndexerError,
        response::{ApiResponse, FinalResponse},
        state::AppState,
    },
    db::entities::rpcs,
};
use axum::{Json, Router, extract::State, routing::get, routing::post};
use sea_orm::EntityTrait;
use sea_orm::LoaderTrait;
use sea_orm::PaginatorTrait;
use sea_orm::QueryOrder;
use sea_orm::QuerySelect;
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use url::{ParseError, Url};

async fn add_rpc(
    state: &AppState,
    http_url: String,
    ws_url: Option<String>,
) -> Result<RpcModel, SvmIndexerError> {
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

    Ok(new_rpc
        .insert(&state.db)
        .await
        .map_err(|err| SvmIndexerError::new_db_error("Failed adding the new RPC!", err))?)
}

async fn add_rpc_handler(
    State(state): State<AppState>,
    Json(payload): Json<AddRpcPayload>,
) -> FinalResponse<RpcModel> {
    let new_rpc = add_rpc(
        &state,
        payload.http_url.trim().to_string(),
        payload.ws_url.map(|u| u.trim().to_string()),
    )
    .await?;

    Ok(ApiResponse::created(new_rpc))
}

async fn get_rpcs(
    State(state): State<AppState>,
    Json(pagination): Json<PaginationPayload>,
) -> FinalResponse<GetRpcsResponse> {
    let rpcs_query = rpcs::Entity::find().order_by_asc(rpcs::Column::Id);

    let (total, pages, rpcs) = match (pagination.per_page, pagination.page) {
        (Some(per_page), Some(page)) => {
            let page = page.max(1);
            let per_page = per_page.clamp(1, 100);
            let rpcs_paginator = rpcs_query.paginate(&state.db, per_page as u64);
            let total = rpcs_paginator.num_items().await.map_err(|err| {
                SvmIndexerError::new_db_error("Failed while fetching pagination info!", err)
            })? as u16;
            let total_pages = rpcs_paginator.num_pages().await.map_err(|err| {
                SvmIndexerError::new_db_error("Failed while fetching pagination info!", err)
            })? as u16;
            let rpcs = rpcs_paginator
                .fetch_page((page - 1) as u64)
                .await
                .map_err(|err| SvmIndexerError::new_db_error("Failed fetching rpcs!", err))?;

            let programs_by_rpc =
                rpcs.load_many(programs::Entity, &state.db)
                    .await
                    .map_err(|err| {
                        SvmIndexerError::new_db_error(
                            "Failed populating and loading related programs data!",
                            err,
                        )
                    })?;
            (
                total,
                total_pages,
                rpcs.into_iter().zip(programs_by_rpc).collect(),
            )
        }
        _ => {
            let result = rpcs_query
                .find_with_related(programs::Entity)
                .all(&state.db)
                .await
                .map_err(|err| SvmIndexerError::new_db_error("Failed fetching rpcs!", err))?;
            (result.len() as u16, 1_u16, result)
        }
    };
    Ok(ApiResponse::ok(GetRpcsResponse::new(rpcs, total, pages)))
}
pub fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/admin/rpcs", post(add_rpc_handler))
        .route("/admin/rpcs", get(get_rpcs))
        // add routes here
        .route_layer(axum::middleware::from_fn_with_state(state, admin_guard))
}
