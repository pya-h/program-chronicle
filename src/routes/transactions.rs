use axum::{
    Router,
    extract::{Path, Query, State},
    routing::get,
};
use sea_orm::{ColumnTrait, Condition, EntityTrait, QueryFilter, QueryOrder};
use serde::Deserialize;
use solana_sdk::signature::Signature;

use crate::{
    core::{
        error::SvmIndexerError,
        response::{ApiResponse, FinalResponse},
        state::AppState,
    },
    db::entities::{slots, transactions},
    routes::{
        payloads::common::PaginationQuery,
        responses::transactions::{TransactionResponse, TransactionsResponse},
    },
};

#[derive(Deserialize)]
struct TransactionsQuery {
    since_slot: Option<i64>,
    from_slot: Option<i64>,
    to_slot: Option<i64>,
    #[serde(flatten)]
    pagination: PaginationQuery,
}

async fn get_transaction_handler(
    State(state): State<AppState>,
    Path(signature): Path<String>,
) -> FinalResponse<TransactionResponse> {
    let signature = signature.parse::<Signature>().map_err(|_| {
        SvmIndexerError::BadInput("Provided signature is not a valid base58 signature!".to_owned())
    })?;
    let found = transactions::Entity::find()
        .filter(transactions::Column::Signature.eq(signature.as_ref().to_vec()))
        .find_also_related(slots::Entity)
        .one(&state.db)
        .await
        .map_err(|err| SvmIndexerError::db_error("Failed fetching transaction!", err))?;

    let Some((tx, slot)) = found else {
        return Err(SvmIndexerError::ItemNotFound(
            "No indexed transaction with such signature!".to_owned(),
        ));
    };
    Ok(ApiResponse::ok(TransactionResponse::from_model(
        tx,
        slot.and_then(|slot| slot.block_time),
    )))
}

async fn get_transactions_handler(
    State(state): State<AppState>,
    Query(query): Query<TransactionsQuery>,
) -> FinalResponse<TransactionsResponse> {
    let mut condition = Condition::all();
    if let Some(since_slot) = query.since_slot {
        condition = condition.add(transactions::Column::Slot.gte(since_slot));
    }
    if let Some(from_slot) = query.from_slot {
        condition = condition.add(transactions::Column::Slot.gte(from_slot));
    }
    if let Some(to_slot) = query.to_slot {
        condition = condition.add(transactions::Column::Slot.lte(to_slot));
    }

    let transactions_query = transactions::Entity::find()
        .filter(condition)
        .find_also_related(slots::Entity)
        .order_by_desc(transactions::Column::Slot)
        .order_by_desc(transactions::Column::Id);

    let (rows, total, pages) = query
        .pagination
        .paginate_or_all(transactions_query, &state.db, Some("Failed fetching transactions!"))
        .await?;
    Ok(ApiResponse::ok(TransactionsResponse::new(
        rows, total, pages,
    )))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(get_transactions_handler))
        .route("/{signature}", get(get_transaction_handler))
}
