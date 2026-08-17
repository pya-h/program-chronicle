use axum::{
    Router,
    extract::{Path, Query, State},
    routing::get,
};
use sea_orm::{
    ColumnTrait, Condition, EntityTrait, QueryFilter, QueryOrder, Value, sea_query::Expr,
};
use solana_sdk::signature::Signature;

use crate::{
    core::{
        commitment::TransactionCommitment,
        error::SvmIndexerError,
        response::{ApiResponse, FinalResponse},
        state::AppState,
    },
    db::entities::{program_accounts, program_events, slots, transactions},
    routes::{
        correlation::correlated_signatures,
        payloads::programs::{ProgramAccountsQuery, ProgramEventsQuery, ProgramTransactionsQuery},
        responses::{
            accounts::ProgramAccountsResponse, events::ProgramEventsResponse,
            transactions::TransactionsResponse,
        },
    },
};

/// Solana's hard account-data cap; a memcmp offset past it can never match, and bounding here keeps
/// `offset + 1` from overflowing the i32 the `substr` expression takes.
const MAX_ACCOUNT_DATA_LEN: i64 = 10 * 1024 * 1024;

fn memcmp_filter(raw: &str) -> Result<Expr, SvmIndexerError> {
    let (offset, encoded) = raw.split_once(':').ok_or_else(|| {
        SvmIndexerError::BadInput("memcmp must be formatted as `<offset>:<base58>`!".to_owned())
    })?;
    let offset = offset
        .parse::<i64>()
        .ok()
        .filter(|off| (0..=MAX_ACCOUNT_DATA_LEN).contains(off))
        .ok_or_else(|| {
            SvmIndexerError::BadInput(
                "memcmp offset must be an integer in [0, 10485760]!".to_owned(),
            )
        })?;
    let bytes = bs58::decode(encoded)
        .into_vec()
        .map_err(|_| SvmIndexerError::BadInput("memcmp bytes must be valid base58!".to_owned()))?;
    if bytes.is_empty() {
        // an empty payload makes `substr(...) = ''` match every row — reject it as a no-op filter
        return Err(SvmIndexerError::BadInput(
            "memcmp needs at least one byte to match!".to_owned(),
        ));
    }
    // substr is 1-indexed, memcmp offset is 0-indexed; substr on bytea needs integer args
    Ok(Expr::cust_with_values(
        "substr(program_accounts.state, $1, $2) = $3",
        [
            Value::from((offset + 1) as i32),
            Value::from(bytes.len() as i32),
            Value::from(bytes),
        ],
    ))
}

async fn get_program_accounts_handler(
    State(state): State<AppState>,
    Path(program_id): Path<i32>,
    Query(query): Query<ProgramAccountsQuery>,
) -> FinalResponse<ProgramAccountsResponse> {
    let mut condition = Condition::all().add(program_accounts::Column::ProgramId.eq(program_id));
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
            "length(program_accounts.state) = $1",
            [Value::from(data_size)],
        ));
    }

    let accounts_query = program_accounts::Entity::find()
        .filter(condition)
        .find_also_related(slots::Entity)
        .order_by_asc(program_accounts::Column::Id);

    let (rows, total, pages) = query
        .pagination
        .paginate_or_all(
            accounts_query,
            &state.db,
            Some("Failed fetching program accounts!"),
        )
        .await?;
    Ok(ApiResponse::ok(ProgramAccountsResponse::new(
        rows, total, pages,
    )))
}

async fn get_program_events_handler(
    State(state): State<AppState>,
    Path(program_id): Path<i32>,
    Query(query): Query<ProgramEventsQuery>,
) -> FinalResponse<ProgramEventsResponse> {
    let mut condition = Condition::all().add(program_events::Column::ProgramId.eq(program_id));
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
    if let Some(discriminator) = &query.discriminator {
        let bytes = hex::decode(discriminator.strip_prefix("0x").unwrap_or(discriminator))
            .ok()
            .filter(|bytes| bytes.len() == 8)
            .ok_or_else(|| {
                SvmIndexerError::BadInput(
                    "discriminator must be the event's leading 8 bytes as hex!".to_owned(),
                )
            })?;
        // keep byte-identical to the expression index so the planner uses it
        condition = condition.add(Expr::cust_with_values(
            "substr(data, 1, 8) = $1",
            [Value::from(bytes)],
        ));
    }

    let events_query = program_events::Entity::find()
        .filter(condition)
        .find_also_related(slots::Entity)
        .order_by_desc(program_events::Column::Slot)
        .order_by_desc(program_events::Column::Id);

    let (rows, total, pages) = query
        .pagination
        .paginate_or_all(
            events_query,
            &state.db,
            Some("Failed fetching program events!"),
        )
        .await?;
    Ok(ApiResponse::ok(ProgramEventsResponse::new(
        rows, total, pages,
    )))
}

async fn get_program_transactions_handler(
    State(state): State<AppState>,
    Path(program_id): Path<i32>,
    Query(query): Query<ProgramTransactionsQuery>,
) -> FinalResponse<TransactionsResponse> {
    let correlated = query.correlated.unwrap_or(false);
    // the derived participation query is a multi-hop join; require a lower bound so it can't scan all
    // history (a lone to_slot still leaves it unbounded below)
    if correlated && query.since_slot.is_none() && query.from_slot.is_none() {
        return Err(SvmIndexerError::BadInput(
            "correlated=true needs a lower slot bound (since_slot or from_slot)!".to_owned(),
        ));
    }

    let mut condition =
        if correlated {
            Condition::all().add(transactions::Column::Signature.in_subquery(
                correlated_signatures(program_id, query.since_slot, query.from_slot, query.to_slot),
            ))
        } else {
            Condition::all().add(transactions::Column::ProgramId.eq(program_id))
        };
    // an explicit ?commitment= matches that level; otherwise reorg-ed `dropped` rows are hidden
    match &query.commitment {
        Some(level) => {
            let level = TransactionCommitment::from_query(level).ok_or_else(|| {
                SvmIndexerError::BadInput(
                    "commitment must be one of processed/confirmed/finalized/dropped!".to_owned(),
                )
            })?;
            condition = condition.add(transactions::Column::Commitment.eq(level.as_str()))
        }
        None => {
            condition = condition
                .add(transactions::Column::Commitment.ne(TransactionCommitment::Dropped.as_str()))
        }
    }
    if let Some(since_slot) = query.since_slot {
        condition = condition.add(transactions::Column::Slot.gte(since_slot));
    }
    if let Some(from_slot) = query.from_slot {
        condition = condition.add(transactions::Column::Slot.gte(from_slot));
    }
    if let Some(to_slot) = query.to_slot {
        condition = condition.add(transactions::Column::Slot.lte(to_slot));
    }
    if let Some(success) = query.success {
        condition = condition.add(transactions::Column::Success.eq(success));
    }

    let transactions_query = transactions::Entity::find()
        .filter(condition)
        .find_also_related(slots::Entity)
        .order_by_desc(transactions::Column::Slot)
        .order_by_desc(transactions::Column::Id);

    let (rows, total, pages) = query
        .pagination
        .paginate_or_all(
            transactions_query,
            &state.db,
            Some("Failed fetching program transactions!"),
        )
        .await?;
    Ok(ApiResponse::ok(TransactionsResponse::new(
        rows, total, pages,
    )))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{program_id}/accounts", get(get_program_accounts_handler))
        .route("/{program_id}/events", get(get_program_events_handler))
        .route(
            "/{program_id}/transactions",
            get(get_program_transactions_handler),
        )
}
