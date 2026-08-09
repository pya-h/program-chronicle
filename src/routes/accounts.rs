use std::collections::HashMap;

use axum::{
    Router,
    extract::{Path, Query, State},
    routing::get,
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::Deserialize;
use solana_sdk::pubkey::Pubkey;

use crate::{
    core::{
        error::SvmIndexerError,
        response::{ApiResponse, FinalResponse},
        state::AppState,
    },
    db::entities::{program_accounts, slots},
    routes::responses::accounts::AccountInfoResponse,
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

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(get_multiple_accounts_handler))
        .route("/{address}", get(get_account_handler))
}
