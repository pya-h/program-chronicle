use sea_orm::{
    ActiveValue::Set, DatabaseConnection, EntityTrait, TransactionTrait, sea_query::OnConflict,
};
use solana_sdk::pubkey::Pubkey;

use crate::core::error::SvmIndexerError;
use crate::db::entities::{programs, transactions};
use crate::indexer::persist::ensure_slot;
use crate::indexer::rpc::{RawTransaction, SvmRpc};

const CATCH_UP_PAGE_SIZE: usize = 1000;

/// Backfills the downtime gap: newest→oldest
pub async fn catch_up_transactions<R: SvmRpc + ?Sized>(
    rpc: &R,
    db: &DatabaseConnection,
    program: &programs::Model,
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
            index_transaction(db, program.id, program.rpc_id, &tx).await?;
        }
        if page_len < CATCH_UP_PAGE_SIZE {
            break;
        }
    }
    Ok(())
}

pub async fn index_transaction(
    db: &DatabaseConnection,
    program_id: i32,
    rpc_id: i32,
    tx: &RawTransaction,
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

    txn.commit()
        .await
        .map_err(|err| SvmIndexerError::db_error("Failed committing transaction!", err))?;
    Ok(())
}
