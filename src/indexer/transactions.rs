use futures::StreamExt;
use sea_orm::{
    ActiveValue::Set, ConnectionTrait, DatabaseConnection, EntityTrait, TransactionTrait,
    sea_query::OnConflict,
};
use solana_sdk::pubkey::Pubkey;
use tokio::sync::broadcast;

use crate::core::commitment::TransactionCommitment;
use crate::core::error::SvmIndexerError;
use crate::core::sse::SseEvent;
use crate::db::entities::{program_events, programs, transaction_accounts, transactions};
use crate::indexer::events::{EventParser, ParsedEvent, RawLogParser};
use crate::indexer::persist::ensure_slot;
use crate::indexer::rpc::{RawTransaction, SvmRpc};

const CATCH_UP_PAGE_SIZE: usize = 1000;

/// Backfills the downtime gap: newest→oldest
pub async fn catch_up_transactions<R: SvmRpc + ?Sized>(
    rpc: &R,
    db: &DatabaseConnection,
    sse_tx: &broadcast::Sender<SseEvent>,
    program: &programs::Model,
    commitment: TransactionCommitment,
) -> Result<(), SvmIndexerError> {
    let address = Pubkey::try_from(program.address.as_slice()).map_err(|_| {
        SvmIndexerError::InvalidPublicKey(
            "Stored program address is not a valid pubkey!".to_owned(),
        )
    })?;
    let floor = program
        .last_processed_slot
        .unwrap_or(0)
        .max(program.start_slot.unwrap_or(0));

    let mut before = None;
    'pages: loop {
        let page = rpc
            .get_signatures_for_address(&address, before, CATCH_UP_PAGE_SIZE)
            .await?;
        let page_len = page.len();
        if page_len == 0 {
            break;
        }
        before = page.last().map(|sig| sig.signature);
        for sig in page {
            if (sig.slot as i64) < floor {
                break 'pages;
            }
            let tx = rpc.get_transaction(&sig.signature).await?;
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
        if page_len < CATCH_UP_PAGE_SIZE {
            break;
        }
    }
    Ok(())
}

pub async fn stream_transactions<R: SvmRpc + ?Sized>(
    rpc: &R,
    db: &DatabaseConnection,
    sse_tx: &broadcast::Sender<SseEvent>,
    program: &programs::Model,
    commitment: TransactionCommitment,
) -> Result<(), SvmIndexerError> {
    let address = Pubkey::try_from(program.address.as_slice()).map_err(|_| {
        SvmIndexerError::InvalidPublicKey(
            "Stored program address is not a valid pubkey!".to_owned(),
        )
    })?;
    let mut stream = rpc.subscribe_logs(&address).await?;

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

    transactions::Entity::insert(transactions::ActiveModel {
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
    .map_err(|err| SvmIndexerError::db_error("Failed persisting transaction!", err))?;

    index_transaction_accounts(&txn, slot, tx, now).await?;

    let events = index_events_from_tx(&txn, program_id, address, slot, slot_id, tx, now).await?;

    txn.commit()
        .await
        .map_err(|err| SvmIndexerError::db_error("Failed committing transaction!", err))?;

    let signature = tx.signature.as_ref().to_vec();
    let _ = sse_tx.send(SseEvent::transaction_indexed(
        program_id,
        signature.clone(),
        slot,
        tx.success,
        tx.err.clone(),
        tx.fee.map(|fee| fee as i64),
        tx.fee_payer.map(|payer| payer.to_bytes().to_vec()),
    ));
    for event in &events {
        let _ = sse_tx.send(SseEvent::program_event(
            program_id,
            signature.clone(),
            event.log_index as i32,
            slot,
            event.data.clone(),
        ));
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
) -> Result<Vec<ParsedEvent>, SvmIndexerError> {
    let events = RawLogParser.parse(address, &tx.logs);
    if events.is_empty() {
        return Ok(events);
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
    program_events::Entity::insert_many(rows)
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
    Ok(events)
}
