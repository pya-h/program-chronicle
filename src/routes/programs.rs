use axum::{
    Router,
    extract::{Path, Query, State},
    routing::get,
};
use sea_orm::{
    ColumnTrait, Condition, EntityTrait, QueryFilter, QueryOrder, Value, sea_query::Expr,
};
use serde::Deserialize;
use solana_sdk::signature::Signature;

use crate::{
    core::{
        error::SvmIndexerError,
        response::{ApiResponse, FinalResponse},
        state::AppState,
    },
    db::entities::{program_accounts, program_events, slots},
    routes::{
        payloads::common::PaginationQuery,
        responses::{accounts::ProgramAccountsResponse, events::ProgramEventsResponse},
    },
};

#[derive(Deserialize)]
struct ProgramAccountsQuery {
    memcmp: Option<String>,
    #[serde(rename = "dataSize")]
    data_size: Option<i64>,
    #[serde(flatten)]
    pagination: PaginationQuery,
}

fn memcmp_filter(raw: &str) -> Result<Expr, SvmIndexerError> {
    let (offset, encoded) = raw.split_once(':').ok_or_else(|| {
        SvmIndexerError::BadInput("memcmp must be formatted as `<offset>:<base58>`!".to_owned())
    })?;
    let offset = offset.parse::<i64>().ok().filter(|off| *off >= 0).ok_or_else(|| {
        SvmIndexerError::BadInput("memcmp offset must be a non-negative integer!".to_owned())
    })?;
    let bytes = bs58::decode(encoded).into_vec().map_err(|_| {
        SvmIndexerError::BadInput("memcmp bytes must be valid base58!".to_owned())
    })?;
    // substr is 1-indexed, memcmp offset is 0-indexed
    Ok(Expr::cust_with_values(
        "substr(program_accounts.state, ?, ?) = ?",
        [
            Value::from(offset + 1),
            Value::from(bytes.len() as i64),
            Value::from(bytes),
        ],
    ))
}

async fn get_program_accounts_handler(
    State(state): State<AppState>,
    Path(program_id): Path<u16>,
    Query(query): Query<ProgramAccountsQuery>,
) -> FinalResponse<ProgramAccountsResponse> {
    let mut condition =
        Condition::all().add(program_accounts::Column::ProgramId.eq(program_id as i32));
    if let Some(memcmp) = &query.memcmp {
        condition = condition.add(memcmp_filter(memcmp)?);
    }
    if let Some(data_size) = query.data_size {
        if data_size < 0 {
            return Err(SvmIndexerError::BadInput(
                "dataSize must be a non-negative integer!".to_owned(),
            ));
        }
        condition = condition.add(Expr::cust_with_values(
            "length(program_accounts.state) = ?",
            [Value::from(data_size)],
        ));
    }

    let accounts_query = program_accounts::Entity::find()
        .filter(condition)
        .find_also_related(slots::Entity)
        .order_by_asc(program_accounts::Column::Id);

    let (rows, total, pages) = query
        .pagination
        .paginate_or_all(accounts_query, &state.db, Some("Failed fetching program accounts!"))
        .await?;
    Ok(ApiResponse::ok(ProgramAccountsResponse::new(
        rows, total, pages,
    )))
}

#[derive(Deserialize)]
struct ProgramEventsQuery {
    since_slot: Option<i64>,
    from_slot: Option<i64>,
    to_slot: Option<i64>,
    tx_signature: Option<String>,
    #[serde(flatten)]
    pagination: PaginationQuery,
}

async fn get_program_events_handler(
    State(state): State<AppState>,
    Path(program_id): Path<u16>,
    Query(query): Query<ProgramEventsQuery>,
) -> FinalResponse<ProgramEventsResponse> {
    let mut condition =
        Condition::all().add(program_events::Column::ProgramId.eq(program_id as i32));
    if let Some(since_slot) = query.since_slot {
        condition = condition.add(program_events::Column::Slot.gte(since_slot));
    }
    if let Some(from_slot) = query.from_slot {
        condition = condition.add(program_events::Column::Slot.gte(from_slot));
    }
    if let Some(to_slot) = query.to_slot {
        condition = condition.add(program_events::Column::Slot.lte(to_slot));
    }
    if let Some(tx_signature) = &query.tx_signature {
        let signature = tx_signature.parse::<Signature>().map_err(|_| {
            SvmIndexerError::BadInput("tx_signature must be a valid base58 signature!".to_owned())
        })?;
        condition =
            condition.add(program_events::Column::TxSignature.eq(signature.as_ref().to_vec()));
    }

    let events_query = program_events::Entity::find()
        .filter(condition)
        .find_also_related(slots::Entity)
        .order_by_desc(program_events::Column::Slot)
        .order_by_desc(program_events::Column::Id);

    let (rows, total, pages) = query
        .pagination
        .paginate_or_all(events_query, &state.db, Some("Failed fetching program events!"))
        .await?;
    Ok(ApiResponse::ok(ProgramEventsResponse::new(
        rows, total, pages,
    )))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{program_id}/accounts", get(get_program_accounts_handler))
        .route("/{program_id}/events", get(get_program_events_handler))
}
