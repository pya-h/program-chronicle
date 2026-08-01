use crate::{
    core::{error::SvmIndexerError, response::FinalResponse, state::AppState}, db::entities::rpcs, routes::{admin::rpcs::add_rpc, payloads::admin::AddProgramPayload, responses::admin::ProgramWithRpcResponse},
};
use axum::{Json, Router, extract::State};
use sea_orm::{ColumnTrait, Condition, EntityTrait, QueryFilter};

async fn add_program_handler(
    State(state): State<AppState>,
    Json(payload): Json<AddProgramPayload>,
) -> FinalResponse<ProgramWithRpcResponse> {
    let rpc = if let Some(rpc_id) = payload.rcp_id {
        let rpc = rpcs::Entity::find_by_id(rpc_id)
            .one(&state.db)
            .await
            .map_err(|err| {
                SvmIndexerError::new_db_error("Failre when trying to fetch RPC info!", err)
            })?;
        let Some(rpc) = rpc else {
            return Err(SvmIndexerError::ItemNotFound(String::from(
                "No RPC found with such id!",
            )));
        };
        rpc
    } else if let Some(rpc) = payload.rpc_url {
        let mut conditions = Condition::all().add(rpcs::Column::HttpUrl.ilike(&rpc.http_url));

        if let Some(ws_url) = &rpc.ws_url {
            conditions = conditions.add(rpcs::Column::WsUrl.ilike(ws_url));
        }
        let maybe_found_rpc = rpcs::Entity::find()
            .filter(conditions)
            .one(&state.db)
            .await
            .map_err(|err| {
                SvmIndexerError::new_db_error(
                    "Something went wrong while checking the rpc info!",
                    err,
                ) // This is not the same with not finding the element! this may be caused by an error, 
                // and the rpc may actually existed in db! So proceeding despite the error is not a good idea!
            })?;

        match maybe_found_rpc {
            Some(found_rpc) => found_rpc,
            _ => add_rpc(&state, rpc.http_url, rpc.ws_url).await?
        }
    } else {
        return Err(SvmIndexerError::BadInput(String::from(
            "You must either specify the rpc_id or rpc url!",
        )));
    };
    todo!()
}

pub fn routes() -> Router<AppState> {
    Router::new()
}
