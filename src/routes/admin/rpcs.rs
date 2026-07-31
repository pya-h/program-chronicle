use super::super::payloads::admin::*;
use super::super::payloads::common::*;
use super::super::responses::admin::*;
use crate::db::entities::programs;
use crate::db::entities::rpcs::Model as RpcModel;
use crate::routes::admin::common;
use crate::{
    core::{
        error::SvmIndexerError,
        response::{ApiResponse, FinalResponse},
        state::AppState,
    },
    db::entities::rpcs,
};
use axum::extract::Path;
use axum::extract::Query;
use axum::{Json, Router, extract::State, routing::get};
use sea_orm::ActiveValue::NotSet;
use sea_orm::EntityTrait;
use sea_orm::LoaderTrait;
use sea_orm::ModelTrait;
use sea_orm::QueryOrder;
use sea_orm::{ActiveModelTrait, ActiveValue::Set};

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
    let http_url = common::validate_http_url(http_url)?;
    let ws_url = if let Some(ws_url) = ws_url {
        common::validate_ws_url(ws_url)?
    } else {
        http_url.replace("http", "ws")
    };
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

async fn get_rpcs_handler(
    State(state): State<AppState>,
    Query(pagination): Query<PaginationQuery>,
) -> FinalResponse<GetRpcsResponse> {
    let rpcs_query = rpcs::Entity::find().order_by_asc(rpcs::Column::Id);

    let (total, pages, rpcs) = match (pagination.per_page, pagination.page) {
        (Some(_), Some(_)) => {
            let (rpcs, total, total_pages) = pagination
                .paginate(rpcs_query, &state.db, Some("Failed fetching rpcs!"))
                .await?;
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

async fn get_single_rpc_handler(
    State(state): State<AppState>,
    Path(rpc_id): Path<u16>,
) -> FinalResponse<RpcWithProgramsResponse> {
    let possible_rpc = rpcs::Entity::find_by_id(rpc_id)
        .one(&state.db)
        .await
        .map_err(|err| SvmIndexerError::new_db_error("Failed fetching the specific RPC!", err))?;
    match possible_rpc {
        Some(rpc) => {
            let programs = rpc
                .find_related(programs::Entity)
                .all(&state.db)
                .await
                .map_err(|err| {
                    SvmIndexerError::new_db_error(
                        "There was a problem while fetching related programs to this rpc!",
                        err,
                    )
                })?;
            Ok(ApiResponse::ok(RpcWithProgramsResponse { rpc, programs }))
        }
        _ => Err(SvmIndexerError::ItemNotFound(String::from("No such RPC!"))),
    }
}

async fn update_rpc_handler(
    State(state): State<AppState>,
    Path(rpc_id): Path<u16>,
    Json(payload): Json<UpdateRpcPayload>,
) -> FinalResponse<RpcModel> {
    let new_http_url = if let Some(hu) = payload.http_url {
        Some(common::validate_http_url(hu)?)
    } else {
        None
    };
    let new_ws_url = if let Some(wu) = payload.ws_url {
        Some(common::validate_ws_url(wu)?)
    } else {
        None
    };
    if new_http_url.is_none() && new_ws_url.is_none() {
        return Err(SvmIndexerError::BadInput(String::from(
            "You must at least provide one field to change!",
        )));
    }
    if rpcs::Entity::find_by_id(rpc_id)
        .one(&state.db)
        .await
        .map_err(|err| SvmIndexerError::new_db_error("Failed accessing the target RPC!", err))?
        .is_none()
    {
        return Err(SvmIndexerError::ItemNotFound(String::from("No such RPC!")));
    }
    let updated_rpc = rpcs::ActiveModel {
        id: Set(rpc_id as i32),
        http_url: new_http_url.map_or(NotSet, |hu| Set(hu)),
        ws_url: new_ws_url.map_or(NotSet, |wu| Set(wu)),
    };

    Ok(ApiResponse::ok(
        updated_rpc.update(&state.db).await.map_err(|err| {
            SvmIndexerError::new_db_error("Something went wrong while updating rpc info!", err)
        })?,
    ))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(get_rpcs_handler).post(add_rpc_handler))
        .route(
            "/{rpc_id}",
            get(get_single_rpc_handler).patch(update_rpc_handler),
        )
}
