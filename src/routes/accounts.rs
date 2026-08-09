use std::collections::HashMap;

use axum::{
    Router,
    extract::{Path, Query, State},
    routing::get,
};
use sea_orm::{ColumnTrait, Condition, EntityTrait, QueryFilter, QueryOrder};
use serde::Deserialize;
use solana_sdk::pubkey::Pubkey;

use crate::{
    core::{
        error::SvmIndexerError,
        response::{ApiResponse, FinalResponse},
        state::AppState,
    },
    db::entities::{account_states, program_accounts, slots},
    routes::{
        payloads::common::PaginationQuery,
        responses::accounts::{AccountHistoryResponse, AccountInfoResponse},
    },
};

fn parse_address(address: &str) -> Result<Vec<u8>, SvmIndexerError> {
    address
        .parse::<Pubkey>()
        .map(|pk| pk.to_bytes().to_vec())
        .map_err(|_| {
            SvmIndexerError::InvalidPublicKey(
                "Provided address is not a standard solana address!".to_owned(),
            )
        })
}

#[derive(Deserialize)]
struct MultiAccountQuery {
    addresses: String,
}

#[derive(Deserialize)]
struct HistoryQuery {
    since_slot: Option<i64>,
    from_slot: Option<i64>,
    to_slot: Option<i64>,
    #[serde(flatten)]
    pagination: PaginationQuery,
}

async fn get_account_handler(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> FinalResponse<AccountInfoResponse> {
    let address = parse_address(&address)?;
    let found = program_accounts::Entity::find()
        .filter(program_accounts::Column::Address.eq(address))
        .find_also_related(slots::Entity)
        .one(&state.db)
        .await
        .map_err(|err| SvmIndexerError::db_error("Failed fetching account info!", err))?;

    let Some((account, slot)) = found else {
        return Err(SvmIndexerError::ItemNotFound(
            "No indexed account with such address!".to_owned(),
        ));
    };
    Ok(ApiResponse::ok(AccountInfoResponse::from_model(
        account,
        slot.and_then(|slot| slot.block_time),
    )))
}

async fn get_multiple_accounts_handler(
    State(state): State<AppState>,
    Query(MultiAccountQuery { addresses }): Query<MultiAccountQuery>,
) -> FinalResponse<Vec<Option<AccountInfoResponse>>> {
    let requested = addresses
        .split(',')
        .map(str::trim)
        .filter(|address| !address.is_empty())
        .map(parse_address)
        .collect::<Result<Vec<_>, _>>()?;

    if requested.is_empty() {
        return Err(SvmIndexerError::BadInput(
            "Provide at least one address in the `addresses` query!".to_owned(),
        ));
    }

    let rows = program_accounts::Entity::find()
        .filter(program_accounts::Column::Address.is_in(requested.clone()))
        .find_also_related(slots::Entity)
        .all(&state.db)
        .await
        .map_err(|err| SvmIndexerError::db_error("Failed fetching accounts info!", err))?;

    let found: HashMap<Vec<u8>, AccountInfoResponse> = rows
        .into_iter()
        .map(|(account, slot)| {
            (
                account.address.clone(),
                AccountInfoResponse::from_model(account, slot.and_then(|slot| slot.block_time)),
            )
        })
        .collect();

    let ordered = requested
        .iter()
        .map(|address| found.get(address).cloned()) // positional, null where an address is not indexed
        .collect();
    Ok(ApiResponse::ok(ordered))
}

async fn get_account_history_handler(
    State(state): State<AppState>,
    Path(address): Path<String>,
    Query(query): Query<HistoryQuery>,
) -> FinalResponse<AccountHistoryResponse> {
    let address = parse_address(&address)?;
    let account = program_accounts::Entity::find()
        .filter(program_accounts::Column::Address.eq(address))
        .one(&state.db)
        .await
        .map_err(|err| SvmIndexerError::db_error("Failed fetching account info!", err))?;

    let Some(account) = account else {
        return Err(SvmIndexerError::ItemNotFound(
            "No indexed account with such address!".to_owned(),
        ));
    };

    let mut condition =
        Condition::all().add(account_states::Column::ProgramAccountId.eq(account.id));
    if let Some(since_slot) = query.since_slot {
        condition = condition.add(account_states::Column::Slot.gte(since_slot));
    }
    if let Some(from_slot) = query.from_slot {
        condition = condition.add(account_states::Column::Slot.gte(from_slot));
    }
    if let Some(to_slot) = query.to_slot {
        condition = condition.add(account_states::Column::Slot.lte(to_slot));
    }

    let history_query = account_states::Entity::find()
        .filter(condition)
        .find_also_related(slots::Entity)
        .order_by_desc(account_states::Column::Slot)
        .order_by_desc(account_states::Column::WriteVersion);

    let (rows, total, pages) = match (query.pagination.per_page, query.pagination.page) {
        (Some(_), Some(_)) => {
            query
                .pagination
                .paginate(history_query, &state.db, Some("Failed fetching account history!"))
                .await?
        }
        _ => {
            let rows = history_query.all(&state.db).await.map_err(|err| {
                SvmIndexerError::db_error("Failed fetching account history!", err)
            })?;
            let total = rows.len() as u16;
            (rows, total, 1_u16)
        }
    };
    Ok(ApiResponse::ok(AccountHistoryResponse::new(
        rows, total, pages,
    )))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(get_multiple_accounts_handler))
        .route("/{address}", get(get_account_handler))
        .route("/{address}/history", get(get_account_history_handler))
}
