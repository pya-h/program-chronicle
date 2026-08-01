use crate::{
    core::{
        error::SvmIndexerError,
        response::{ApiResponse, FinalResponse},
        state::AppState,
    },
    db::entities::{programs, rpcs},
    routes::{
        admin::rpcs::add_rpc,
        payloads::{
            admin::{AddProgramPayload, GetSingleProgramPayload},
            common::PaginationQuery,
        },
        responses::admin::{GetProgramsResponse, ProgramWithRpcResponse},
    },
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use sea_orm::{
    ActiveModelTrait,
    ActiveValue::{NotSet, Set},
    ColumnTrait, Condition, EntityTrait, QueryFilter, QueryOrder,
};
use solana_sdk::pubkey::Pubkey;

async fn add_program_handler(
    State(state): State<AppState>,
    Json(payload): Json<AddProgramPayload>,
) -> FinalResponse<ProgramWithRpcResponse> {
    let rpc = if let Some(rpc_id) = payload.rpc_id {
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
            _ => add_rpc(&state, rpc.http_url, rpc.ws_url).await?,
        }
    } else {
        return Err(SvmIndexerError::BadInput(String::from(
            "You must either specify the rpc_id or rpc url!",
        )));
    };
    let address = payload.address.parse::<Pubkey>().map_err(|_| {
        SvmIndexerError::InvalidPublicKey(String::from(
            "Provided address is not a standard solana address!",
        ))
    })?;
    let program = programs::ActiveModel {
        rpc_id: Set(rpc.id),
        address: Set(address.to_bytes().to_vec()),
        last_processed_slot: payload.start_slot.map_or(NotSet, |ss| Set(Some(ss as i64))),
        name: payload.name.map_or(NotSet, |name| Set(Some(name))),
        ..Default::default()
    };

    Ok(ApiResponse::created(ProgramWithRpcResponse {
        program: program.insert(&state.db).await.map_err(|err| {
            SvmIndexerError::new_db_error(
                r#"Adding the new program failed at last step! 
                Maybe you've already registered this program on this rpc?"#,
                err,
            )
        })?,
        rpc,
    }))
}

async fn get_single_program_handler(
    State(state): State<AppState>,
    Path(program_id): Path<u16>,
    Query(GetSingleProgramPayload { ignore_unknown_rpc }): Query<GetSingleProgramPayload>,
) -> FinalResponse<ProgramWithRpcResponse> {
    let possible_program = programs::Entity::find_by_id(program_id)
        .find_also_related(rpcs::Entity)
        .one(&state.db)
        .await
        .map_err(|err| SvmIndexerError::new_db_error("Failed fetching program info!", err))?;

    match possible_program {
        Some((program, possible_rpc)) => {
            if let Some(rpc) = possible_rpc {
                Ok(ApiResponse::ok(ProgramWithRpcResponse { program, rpc }))
            } else {
                if let Some(unknown_rpc) = ignore_unknown_rpc
                    && unknown_rpc
                {
                    Ok(ApiResponse::ok(ProgramWithRpcResponse::new_no_rpc(program)))
                } else {
                    Err(SvmIndexerError::InternalError(String::from(
                        "It seems there was a conflict while trying to fetch program's RPC info!",
                    )))
                }
            }
        }
        _ => Err(SvmIndexerError::ItemNotFound(String::from(
            "No such program!",
        ))),
    }
}

async fn get_programs_handler(
    State(state): State<AppState>,
    Query(pagination): Query<PaginationQuery>,
) -> FinalResponse<GetProgramsResponse> {
    let programs_query = programs::Entity::find()
        .find_also_related(rpcs::Entity)
        .order_by_asc(programs::Column::Id);

    let (programs, total, pages) = match (pagination.per_page, pagination.page) {
        (Some(_), Some(_)) => {
            pagination
                .paginate(programs_query, &state.db, Some("Failed fetching programs!"))
                .await?
        }
        _ => {
            let programs = programs_query
                .all(&state.db)
                .await
                .map_err(|err| SvmIndexerError::new_db_error("Failed fetching programs!", err))?;
            let total = programs.len() as u16;
            (programs, total, 1_u16)
        }
    };
    Ok(ApiResponse::ok(GetProgramsResponse::new(
        programs,
        total as u16,
        pages,
    )))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(get_programs_handler).post(add_program_handler))
        .route("/{program_id}", get(get_single_program_handler))
}
