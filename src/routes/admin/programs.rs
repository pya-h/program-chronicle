use crate::indexer::rpc::AccountFilter;
use crate::{
    core::{
        error::SvmIndexerError,
        response::{ApiResponse, FinalResponse, NoContentResponse},
        state::{AppState, ProgramCommand},
    },
    db::entities::{
        programs,
        rpcs::{self, Model as RpcModel},
    },
    routes::{
        admin::rpcs::add_rpc,
        payloads::{
            admin::{
                AddProgramPayload, AddRpcPayload, GetSingleProgramPayload, UpdateProgramPayload,
            },
            common::PaginationQuery,
        },
        responses::admin::{GetProgramsResponse, ProgramWithRpcResponse},
    },
    unknown_rpc,
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    response::NoContent,
    routing::{get, post},
};
use sea_orm::{
    ActiveModelTrait, ActiveValue,
    ActiveValue::{NotSet, Set, Unchanged},
    ColumnTrait, Condition, EntityTrait, ModelTrait, QueryFilter, QueryOrder,
};
use solana_sdk::pubkey::Pubkey;

/// Validates a filter descriptor and turns it into the `account_filters` column value: an empty or
/// absent descriptor keeps the column untouched, a populated one stores its JSON.
fn account_filters_value(
    filter: Option<AccountFilter>,
) -> Result<ActiveValue<Option<serde_json::Value>>, SvmIndexerError> {
    match filter {
        Some(filter) if !filter.is_empty() => {
            filter.validate()?;
            let json = serde_json::to_value(&filter)
                .map_err(|err| SvmIndexerError::SerializationError(err.to_string()))?;
            Ok(Set(Some(json)))
        }
        Some(_) => Ok(Set(None)), // an explicit empty descriptor clears the filter (track-all)
        None => Ok(NotSet),
    }
}

/// A `memcmp` never matches past the end of an account's data, so an offset beyond the largest
/// account the runtime allows can only be a typo.
const MAX_SCAN_PARTITION_OFFSET: i32 = 10 * 1024 * 1024;

fn scan_partition_offset_value(
    offset: Option<Option<i32>>,
) -> Result<ActiveValue<Option<i32>>, SvmIndexerError> {
    match offset {
        Some(Some(offset)) if !(0..=MAX_SCAN_PARTITION_OFFSET).contains(&offset) => {
            Err(SvmIndexerError::BadInput(format!(
                "scan_partition_offset must be between 0 and {MAX_SCAN_PARTITION_OFFSET}!"
            )))
        }
        Some(offset) => Ok(Set(offset)),
        None => Ok(NotSet),
    }
}

async fn get_or_add_rpc(
    state: &AppState,
    rpc_id: &Option<i32>,
    rpc_url: &Option<AddRpcPayload>,
) -> Result<Option<rpcs::Model>, SvmIndexerError> {
    if let Some(rpc_id) = *rpc_id {
        let rpc = rpcs::Entity::find_by_id(rpc_id)
            .one(&state.db)
            .await
            .map_err(|err| {
                SvmIndexerError::db_error_with_id(
                    "Failre when trying to fetch RPC info!",
                    err,
                    rpc_id as u64,
                )
            })?;
        let Some(rpc) = rpc else {
            return Err(SvmIndexerError::ItemNotFound(String::from(
                "No RPC found with such id!",
            )));
        };
        Ok(Some(rpc))
    } else if let Some(rpc) = rpc_url {
        let mut conditions = Condition::all().add(rpcs::Column::HttpUrl.eq(&rpc.http_url));

        if let Some(ws_url) = &rpc.ws_url {
            conditions = conditions.add(rpcs::Column::WsUrl.eq(ws_url));
        }
        let maybe_found_rpc = rpcs::Entity::find()
            .filter(conditions)
            .one(&state.db)
            .await
            .map_err(|err| {
                SvmIndexerError::db_error("Something went wrong while checking the rpc info!", err) // This is not the same with not finding the element! this may be caused by an error, 
                // and the rpc may actually existed in db! So proceeding despite the error is not a good idea!
            })?;

        Ok(Some(match maybe_found_rpc {
            Some(found_rpc) => found_rpc,
            _ => add_rpc(state, rpc.http_url.clone(), rpc.ws_url.clone()).await?,
        }))
    } else {
        Ok(None)
    }
}

async fn add_program_handler(
    State(state): State<AppState>,
    Json(payload): Json<AddProgramPayload>,
) -> FinalResponse<ProgramWithRpcResponse> {
    let Some(rpc) = get_or_add_rpc(&state, &payload.rpc_id, &payload.rpc_url).await? else {
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
        start_slot: payload.start_slot.map_or(NotSet, |ss| Set(Some(ss as i64))),
        name: payload.name.map_or(NotSet, |name| Set(Some(name))),
        account_filters: account_filters_value(payload.account_filters)?,
        scan_partition_offset: scan_partition_offset_value(
            payload.scan_partition_offset.map(Some),
        )?,
        ..Default::default()
    };

    let program = program.insert(&state.db).await.map_err(|err| {
        SvmIndexerError::db_error_with_id(
            "Adding the new program failed at last step! Maybe the program already registered on this rpc?",
            err,
            rpc.id as u64
        )
    })?;
    if let Some(tx) = &state.supervisor_tx {
        // The supervisor retries the spawned indexer run with backoff (see spawn_program); a failed
        // send here only means the supervisor is gone, which a restart resolves.
        let _ = tx.send(ProgramCommand::Start(program.id)).await;
    }

    Ok(ApiResponse::created(ProgramWithRpcResponse {
        program,
        rpc,
    }))
}

/// Flips a program's `active` flag and tells the supervisor to start or stop its indexer task.
async fn set_program_activation(
    state: &AppState,
    program_id: i32,
    active: bool,
) -> FinalResponse<ProgramWithRpcResponse> {
    let found = programs::Entity::find_by_id(program_id)
        .find_also_related(rpcs::Entity)
        .one(&state.db)
        .await
        .map_err(|err| {
            SvmIndexerError::db_error_with_id(
                "Failed fetching program info!",
                err,
                program_id as u64,
            )
        })?;
    let Some((program, rpc)) = found else {
        return Err(SvmIndexerError::ItemNotFound(String::from(
            "No such program!",
        )));
    };

    if program.active != active {
        programs::ActiveModel {
            id: Unchanged(program.id),
            active: Set(active),
            ..Default::default()
        }
        .update(&state.db)
        .await
        .map_err(|err| {
            SvmIndexerError::db_error_with_id(
                "Failed updating the program activation state!",
                err,
                program_id as u64,
            )
        })?;
        if let Some(tx) = &state.supervisor_tx {
            let command = if active {
                ProgramCommand::Start(program.id)
            } else {
                ProgramCommand::Stop(program.id)
            };
            let _ = tx.send(command).await;
        }
    }

    Ok(ApiResponse::ok(ProgramWithRpcResponse {
        program: programs::Model { active, ..program },
        rpc: rpc.unwrap_or(unknown_rpc!()),
    }))
}

async fn activate_program_handler(
    State(state): State<AppState>,
    Path(program_id): Path<i32>,
) -> FinalResponse<ProgramWithRpcResponse> {
    set_program_activation(&state, program_id, true).await
}

async fn deactivate_program_handler(
    State(state): State<AppState>,
    Path(program_id): Path<i32>,
) -> FinalResponse<ProgramWithRpcResponse> {
    set_program_activation(&state, program_id, false).await
}

async fn get_single_program_handler(
    State(state): State<AppState>,
    Path(program_id): Path<i32>,
    Query(GetSingleProgramPayload { ignore_unknown_rpc }): Query<GetSingleProgramPayload>,
) -> FinalResponse<ProgramWithRpcResponse> {
    let possible_program = programs::Entity::find_by_id(program_id)
        .find_also_related(rpcs::Entity)
        .one(&state.db)
        .await
        .map_err(|err| {
            SvmIndexerError::db_error_with_id(
                "Failed fetching program info!",
                err,
                program_id as u64,
            )
        })?;

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

    let (programs, total, pages) = pagination
        .paginate_or_all(programs_query, &state.db, Some("Failed fetching programs!"))
        .await?;
    Ok(ApiResponse::ok(GetProgramsResponse::new(
        programs, total, pages,
    )))
}

async fn update_program_handler(
    State(state): State<AppState>,
    Path(program_id): Path<i32>,
    Json(payload): Json<UpdateProgramPayload>,
) -> FinalResponse<ProgramWithRpcResponse> {
    if payload.is_empty() {
        return Err(SvmIndexerError::BadInput(String::from(
            "At least one field of program should be changed!",
        )));
    }

    let found = programs::Entity::find_by_id(program_id)
        .one(&state.db)
        .await
        .map_err(|err| {
            SvmIndexerError::db_error_with_id(
                "Failed accessing the target program data!",
                err,
                program_id as u64,
            )
        })?;
    let Some(current) = found else {
        return Err(SvmIndexerError::ItemNotFound(
            "Program not found!".to_owned(),
        ));
    };
    let address = if let Some(address) = payload.address {
        Set(address
            .parse::<Pubkey>()
            .map_err(|_| {
                SvmIndexerError::InvalidPublicKey(
                    "Provided address is not a standard solana address!".to_owned(),
                )
            })?
            .to_bytes()
            .to_vec())
    } else {
        NotSet
    };
    let changed_rpc = get_or_add_rpc(&state, &payload.rpc_id, &payload.rpc_url).await?;
    // both slot fields are numbered in the old network's space; carried onto a shorter chain they
    // leave the catch-up floor above every signature the program will ever see there
    let repointed = changed_rpc
        .as_ref()
        .is_some_and(|rpc| rpc.id != current.rpc_id);
    let program = programs::ActiveModel {
        id: Unchanged(program_id),
        address,
        name: payload.name.map_or(NotSet, |name| {
            let new_name = name.trim().to_owned();
            if new_name.is_empty() {
                Set(None) // User wants to clear previousely set name.
            } else {
                Set(Some(new_name))
            }
        }),
        rpc_id: changed_rpc.clone().map_or(NotSet, |nr| Set(nr.id)),
        start_slot: if repointed { Set(None) } else { NotSet },
        last_processed_slot: if repointed { Set(None) } else { NotSet },
        last_processed_at: if repointed { Set(None) } else { NotSet },
        account_filters: account_filters_value(payload.account_filters)?,
        scan_partition_offset: scan_partition_offset_value(payload.scan_partition_offset)?,
        ..Default::default()
    }
    .update(&state.db)
    .await
    .map_err(|err| {
        SvmIndexerError::db_error_with_id(
            "Failed updating the program data!",
            err,
            program_id as u64,
        )
    })?;

    // without this the old task keeps ticking on the old client and rewrites the cursor just cleared
    if repointed && let Some(tx) = &state.supervisor_tx {
        let _ = tx.send(ProgramCommand::Stop(program_id)).await;
        let _ = tx.send(ProgramCommand::Start(program_id)).await;
    }

    if let Some(rpc) = changed_rpc {
        return Ok(ApiResponse::ok(ProgramWithRpcResponse { program, rpc }));
    }
    let rpc_loaded = program.find_related(rpcs::Entity).one(&state.db).await;
    // Since the update action has already happened successfully, throwing error status doesnt seem rational
    let final_rpc_id = program.rpc_id; // prevent move errors on error handling section below!
    Ok(ApiResponse::ok(ProgramWithRpcResponse {
        program,
        rpc: if let Ok(rpc_maybe) = &rpc_loaded
            && let Some(rpc) = rpc_maybe
        {
            rpc.clone()
        } else {
            if let Err(err) = &rpc_loaded {
                tracing::error!(
                    error = %err,
                    message = "Updating program data was successful, but loading rpc info failed!",
                    program_id = program_id,
                    rpc_id = final_rpc_id
                );
            } else {
                tracing::error!(
                    message = "Updating program data was successful, but rpc not found!",
                    program_id = program_id,
                    rpc_id = final_rpc_id
                );
            }
            unknown_rpc!()
        },
    }))
}

async fn delete_program_handler(
    State(state): State<AppState>,
    Path(program_id): Path<i32>,
) -> NoContentResponse {
    if programs::Entity::delete_by_id(program_id)
        .exec(&state.db)
        .await
        .map_err(|err| {
            SvmIndexerError::db_error_with_id(
                "Something went wrong while deleting this program.",
                err,
                program_id as u64,
            )
        })?
        .rows_affected
        == 0
    {
        return Err(SvmIndexerError::ItemNotFound(
            "No such program found to delete".to_owned(),
        ));
    }
    // signalled after the row is gone, so a delete the foreign keys reject leaves its task running
    if let Some(tx) = &state.supervisor_tx {
        let _ = tx.send(ProgramCommand::Stop(program_id)).await;
    }
    Ok(NoContent)
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(get_programs_handler).post(add_program_handler))
        .route(
            "/{program_id}",
            get(get_single_program_handler)
                .patch(update_program_handler)
                .delete(delete_program_handler),
        )
        .route("/{program_id}/activate", post(activate_program_handler))
        .route("/{program_id}/deactivate", post(deactivate_program_handler))
}
