use sea_orm::{
    ActiveValue::Set, DatabaseConnection, EntityTrait, TransactionTrait, sea_query::OnConflict,
};

use crate::core::error::SvmIndexerError;
use crate::db::entities::transactions;
use crate::indexer::persist::ensure_slot;
use crate::indexer::rpc::RawTransaction;

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
