use std::collections::HashSet;

use futures::StreamExt;
use sea_orm::{
    ActiveValue::Set,
    ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QuerySelect, TransactionTrait,
    sea_query::{Expr, OnConflict},
};
use solana_sdk::{pubkey::Pubkey, signature::Signature};
use tokio::sync::broadcast;

use crate::core::commitment::TransactionCommitment;
use crate::core::error::SvmIndexerError;
use crate::core::sse::SseEvent;
use crate::db::entities::{program_events, programs, transaction_accounts, transactions};
use crate::indexer::events::{EventParser, ParsedEvent, RawLogParser};
use crate::indexer::persist::ensure_slot;
use crate::indexer::rpc::{RawTransaction, SignatureInfo, SubReady, SvmRpc};

const CATCH_UP_PAGE_SIZE: usize = 1000;
const TRANSACTION_BATCH_SIZE: usize = 25;

/// Where the backward walk stops. `since` raises the floor above the program cursor, for the narrow
/// re-sweep that closes the window between the bulk walk and the live subscription.
pub(crate) fn catch_up_floor(program: &programs::Model, since: Option<i64>) -> i64 {
    since
        .or(program.last_processed_slot)
        .unwrap_or(0)
        .max(program.start_slot.unwrap_or(0))
}

/// Backfills the downtime gap: newest→oldest
pub async fn catch_up_transactions<R: SvmRpc + ?Sized>(
    rpc: &R,
    db: &DatabaseConnection,
    sse_tx: &broadcast::Sender<SseEvent>,
    program: &programs::Model,
    commitment: TransactionCommitment,
    since: Option<i64>,
) -> Result<(), SvmIndexerError> {
    let address = Pubkey::try_from(program.address.as_slice()).map_err(|_| {
        SvmIndexerError::InvalidPublicKey(
            "Stored program address is not a valid pubkey!".to_owned(),
        )
    })?;
    let floor = catch_up_floor(program, since);
    let track = since.is_none();

    if track && let Some(cursor) = resume_cursor(program) {
        // pages arrive newest-first, so a gap that keeps growing at the top would starve the walk of
        // its own tail forever. Finish where the last run died before looking at anything above it.
        walk(
            rpc,
            db,
            sse_tx,
            program,
            commitment,
            &address,
            floor,
            Some(cursor),
            true,
        )
        .await?;
    }
    walk(
        rpc, db, sse_tx, program, commitment, &address, floor, None, track,
    )
    .await
}

/// The signature an interrupted walk should page on from, if one was recorded.
pub(crate) fn resume_cursor(program: &programs::Model) -> Option<Signature> {
    Signature::try_from(program.backfill_cursor.as_deref()?).ok()
}

#[allow(clippy::too_many_arguments)]
async fn walk<R: SvmRpc + ?Sized>(
    rpc: &R,
    db: &DatabaseConnection,
    sse_tx: &broadcast::Sender<SseEvent>,
    program: &programs::Model,
    commitment: TransactionCommitment,
    address: &Pubkey,
    floor: i64,
    from: Option<Signature>,
    track: bool,
) -> Result<(), SvmIndexerError> {
    let mut before = from;
    loop {
        let page = rpc
            .get_signatures_for_address(address, before, CATCH_UP_PAGE_SIZE)
            .await?;
        let page_len = page.len();
        if page_len == 0 {
            break;
        }
        before = page.last().map(|sig| sig.signature);
        let stored = stored_signatures(db, program.id, &page, floor).await?;

        let mut reached_floor = false;
        let mut missing = Vec::new();
        for sig in &page {
            if (sig.slot as i64) < floor {
                reached_floor = true;
                break;
            }
            if !stored.contains(sig.signature.as_ref()) {
                missing.push(sig.signature);
            }
        }

        for chunk in missing.chunks(TRANSACTION_BATCH_SIZE) {
            for tx in rpc.get_transactions(chunk).await?.into_iter().flatten() {
                index_transaction(
                    db,
                    sse_tx,
                    program.id,
                    program.rpc_id,
                    address,
                    &tx,
                    commitment,
                )
                .await?;
            }
        }

        if reached_floor || page_len < CATCH_UP_PAGE_SIZE {
            break;
        }
        if track && let (Some(cursor), Some(oldest)) = (before, page.last().map(|sig| sig.slot)) {
            // this page is stored end to end, so a restart can page on from strictly below it
            record_backfill_frontier(db, program.id, oldest as i64, &cursor).await?;
        }
    }
    if track {
        clear_backfill_frontier(db, program.id).await?;
    }
    Ok(())
}

async fn record_backfill_frontier(
    db: &DatabaseConnection,
    program_id: i32,
    slot: i64,
    cursor: &Signature,
) -> Result<(), SvmIndexerError> {
    programs::Entity::update_many()
        .col_expr(programs::Column::BackfillSlot, Expr::value(slot))
        .col_expr(
            programs::Column::BackfillCursor,
            Expr::value(cursor.as_ref().to_vec()),
        )
        .filter(programs::Column::Id.eq(program_id))
        .filter(
            Condition::any()
                .add(programs::Column::BackfillSlot.is_null())
                .add(programs::Column::BackfillSlot.gt(slot)),
        )
        .exec(db)
        .await
        .map_err(|err| SvmIndexerError::db_error("Failed recording backfill progress!", err))?;
    Ok(())
}

async fn clear_backfill_frontier(
    db: &DatabaseConnection,
    program_id: i32,
) -> Result<(), SvmIndexerError> {
    programs::Entity::update_many()
        .col_expr(
            programs::Column::BackfillSlot,
            Expr::value(Option::<i64>::None),
        )
        .col_expr(
            programs::Column::BackfillCursor,
            Expr::value(Option::<Vec<u8>>::None),
        )
        .filter(programs::Column::Id.eq(program_id))
        .exec(db)
        .await
        .map_err(|err| SvmIndexerError::db_error("Failed clearing backfill progress!", err))?;
    Ok(())
}

/// Which of a page's signatures this program already stored, so the walk spends a `getTransaction`
/// only on what is missing.
pub async fn stored_signatures(
    db: &DatabaseConnection,
    program_id: i32,
    page: &[SignatureInfo],
    floor: i64,
) -> Result<HashSet<Vec<u8>>, SvmIndexerError> {
    let keys: Vec<Vec<u8>> = page
        .iter()
        .filter(|sig| (sig.slot as i64) >= floor)
        .map(|sig| sig.signature.as_ref().to_vec())
        .collect();
    if keys.is_empty() {
        return Ok(HashSet::new());
    }
    transactions::Entity::find()
        .select_only()
        .column(transactions::Column::Signature)
        .filter(transactions::Column::ProgramId.eq(program_id))
        .filter(transactions::Column::Signature.is_in(keys))
        .into_tuple::<Vec<u8>>()
        .all(db)
        .await
        .map(HashSet::from_iter)
        .map_err(|err| SvmIndexerError::db_error("Failed checking indexed signatures!", err))
}

pub async fn stream_transactions<R: SvmRpc + ?Sized>(
    rpc: &R,
    db: &DatabaseConnection,
    sse_tx: &broadcast::Sender<SseEvent>,
    program: &programs::Model,
    commitment: TransactionCommitment,
    ready: SubReady,
) -> Result<(), SvmIndexerError> {
    let address = Pubkey::try_from(program.address.as_slice()).map_err(|_| {
        SvmIndexerError::InvalidPublicKey(
            "Stored program address is not a valid pubkey!".to_owned(),
        )
    })?;
    let mut stream = rpc.subscribe_logs(&address, ready).await?;

    while let Some(notification) = stream.next().await {
        let notification = notification?;
        let tx = rpc.get_transaction(&notification.signature).await?;
        index_transaction(
            db,
            sse_tx,
            program.id,
            program.rpc_id,
            &address,
            &tx,
            commitment,
        )
        .await?;
    }
    // a live subscription never ends on its own; a closed stream means the connection dropped
    Err(SvmIndexerError::SubscriptionFailed(
        "transaction log subscription ended".to_owned(),
    ))
}

pub async fn index_transaction(
    db: &DatabaseConnection,
    sse_tx: &broadcast::Sender<SseEvent>,
    program_id: i32,
    rpc_id: i32,
    address: &Pubkey,
    tx: &RawTransaction,
    commitment: TransactionCommitment,
) -> Result<(), SvmIndexerError> {
    let now = chrono::Utc::now().naive_utc();
    let slot = tx.slot as i64;

    let txn = db
        .begin()
        .await
        .map_err(|err| SvmIndexerError::db_error("Failed opening transaction!", err))?;

    let slot_id = ensure_slot(&txn, rpc_id, slot, tx.block_time).await?;

    let tx_inserted = transactions::Entity::insert(transactions::ActiveModel {
        signature: Set(tx.signature.as_ref().to_vec()),
        program_id: Set(Some(program_id)),
        slot: Set(slot),
        slot_id: Set(slot_id),
        success: Set(tx.success),
        error: Set(tx.err.clone()),
        commitment: Set(commitment.as_str().to_owned()),
        fee: Set(tx.fee.map(|fee| fee as i64)),
        fee_payer: Set(tx.fee_payer.map(|payer| payer.to_bytes().to_vec())),
        raw: Set(tx.raw.clone()),
        created_at: Set(now),
        ..Default::default()
    })
    .on_conflict(
        OnConflict::column(transactions::Column::Signature)
            .do_nothing()
            .to_owned(),
    )
    .exec_without_returning(&txn)
    .await
    .map_err(|err| SvmIndexerError::db_error("Failed persisting transaction!", err))?
        > 0;

    index_transaction_accounts(&txn, slot, tx, now).await?;

    let (events, events_inserted) =
        index_events_from_tx(&txn, program_id, address, slot, slot_id, tx, now).await?;

    txn.commit()
        .await
        .map_err(|err| SvmIndexerError::db_error("Failed committing transaction!", err))?;

    // skip re-broadcasting a re-ingested tx (catch-up/live overlap); a second program's genuinely
    // new events still fire on their own insert
    let signature = tx.signature.as_ref().to_vec();
    if tx_inserted {
        let _ = sse_tx.send(SseEvent::transaction_indexed(
            program_id,
            signature.clone(),
            slot,
            tx.success,
            tx.err.clone(),
            tx.fee.map(|fee| fee as i64),
            tx.fee_payer.map(|payer| payer.to_bytes().to_vec()),
        ));
    }
    if events_inserted {
        for event in &events {
            let _ = sse_tx.send(SseEvent::program_event(
                program_id,
                signature.clone(),
                event.log_index as i32,
                slot,
                event.data.clone(),
            ));
        }
    }
    Ok(())
}

/// Records every account the transaction references so an address can be mapped back to its transactions
async fn index_transaction_accounts<C: ConnectionTrait>(
    db: &C,
    slot: i64,
    tx: &RawTransaction,
    now: chrono::NaiveDateTime,
) -> Result<(), SvmIndexerError> {
    if tx.accounts.is_empty() {
        return Ok(());
    }
    let signature = tx.signature.as_ref().to_vec();
    let rows = tx
        .accounts
        .iter()
        .map(|account| transaction_accounts::ActiveModel {
            tx_signature: Set(signature.clone()),
            account_address: Set(account.address.to_bytes().to_vec()),
            slot: Set(slot),
            writable: Set(account.writable),
            created_at: Set(now),
            ..Default::default()
        });
    transaction_accounts::Entity::insert_many(rows)
        .on_conflict(
            OnConflict::columns([
                transaction_accounts::Column::TxSignature,
                transaction_accounts::Column::AccountAddress,
            ])
            .do_nothing()
            .to_owned(),
        )
        .exec_without_returning(db)
        .await
        .map_err(|err| SvmIndexerError::db_error("Failed persisting transaction accounts!", err))?;
    Ok(())
}

async fn index_events_from_tx<C: ConnectionTrait>(
    db: &C,
    program_id: i32,
    address: &Pubkey,
    slot: i64,
    slot_id: i64,
    tx: &RawTransaction,
    now: chrono::NaiveDateTime,
) -> Result<(Vec<ParsedEvent>, bool), SvmIndexerError> {
    let events = RawLogParser.parse(address, &tx.logs);
    if events.is_empty() {
        return Ok((events, false));
    }
    let signature = tx.signature.as_ref().to_vec();
    let rows = events.iter().map(|event| program_events::ActiveModel {
        program_id: Set(program_id),
        tx_signature: Set(signature.clone()),
        slot: Set(slot),
        slot_id: Set(slot_id),
        log_index: Set(event.log_index as i32),
        data: Set(event.data.clone()),
        created_at: Set(now),
        ..Default::default()
    });
    let inserted = program_events::Entity::insert_many(rows)
        .on_conflict(
            OnConflict::columns([
                program_events::Column::TxSignature,
                program_events::Column::LogIndex,
            ])
            .do_nothing()
            .to_owned(),
        )
        .exec_without_returning(db)
        .await
        .map_err(|err| SvmIndexerError::db_error("Failed persisting program events!", err))?;
    Ok((events, inserted > 0))
}
