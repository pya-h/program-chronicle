use std::sync::Arc;
use std::time::Duration;

use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, sea_query::Expr,
};
use solana_sdk::signature::Signature;

use crate::core::commitment::TransactionCommitment;
use crate::core::error::SvmIndexerError;
use crate::db::entities::{programs, rpcs, slots, transactions};
use crate::indexer::Indexer;
use crate::indexer::persist::ensure_slot;

const BACKFILL_BATCH: u64 = 500;
/// Cap the work per tx_signature backfill pass; leftovers are picked up on the next tick.
const TX_SIGNATURE_BACKFILL_MAX_BATCHES: usize = 40;
/// `getSignatureStatuses` caps at 256 signatures per call.
const STATUS_BATCH: u64 = 256;
/// Only sweep transactions within this many slots of the head — inside the node's recent status-cache
/// retention, so an absent signature is a reorg drop, not an aged-out finalized tx.
const COMMITMENT_RECENT_SLOTS: i64 = 150;

pub async fn backfill_tx_signatures_once(
    db: &DatabaseConnection,
) -> Result<u64, SvmIndexerError> {
    // the per-batch LIMIT only selects groups that still have a NULL state, so each pass makes progress
    const SQL: &str = "\
        UPDATE account_states s \
        SET tx_signature = m.sig, updated_at = CURRENT_TIMESTAMP \
        FROM ( \
            SELECT pa.id AS program_account_id, ta.slot AS slot, \
                   (ARRAY_AGG(DISTINCT ta.tx_signature))[1] AS sig \
            FROM transaction_accounts ta \
            JOIN program_accounts pa ON pa.address = ta.account_address \
            WHERE ta.writable \
              AND EXISTS ( \
                  SELECT 1 FROM account_states s2 \
                  WHERE s2.program_account_id = pa.id AND s2.slot = ta.slot \
                    AND s2.tx_signature IS NULL \
              ) \
            GROUP BY pa.id, ta.slot \
            HAVING COUNT(DISTINCT ta.tx_signature) = 1 \
            LIMIT 500 \
        ) m \
        WHERE s.program_account_id = m.program_account_id AND s.slot = m.slot \
          AND s.tx_signature IS NULL";

    let mut filled = 0u64;
    for _ in 0..TX_SIGNATURE_BACKFILL_MAX_BATCHES {
        let result = db.execute_unprepared(SQL).await.map_err(|err| {
            SvmIndexerError::db_error("Failed backfilling account_states.tx_signature!", err)
        })?;
        let affected = result.rows_affected();
        filled += affected;
        if affected == 0 {
            break;
        }
    }
    Ok(filled)
}

impl Indexer {
    /// Fills `slots.block_time` left NULL at ingest.
    pub async fn run_block_time_backfill(self: Arc<Self>, interval: Duration) {
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            if let Err(err) = self.backfill_block_times_once().await {
                tracing::error!(error = %err, "block_time backfill pass failed!");
            }
        }
    }

    pub async fn run_tx_signature_backfill(self: Arc<Self>, interval: Duration) {
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            match backfill_tx_signatures_once(&self.db).await {
                Ok(filled) if filled > 0 => {
                    tracing::info!(filled, "tx_signature backfill attributed account states")
                }
                Ok(_) => {}
                Err(err) => tracing::error!(error = %err, "tx_signature backfill pass failed!"),
            }
        }
    }

    pub async fn run_commitment_worker(self: Arc<Self>, interval: Duration) {
        if !self.ingests_confirmed() {
            return;
        }
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            if let Err(err) = self.promote_commitments_once().await {
                tracing::error!(error = %err, "commitment promote/sweep pass failed!");
            }
        }
    }

    async fn promote_commitments_once(&self) -> Result<(), SvmIndexerError> {
        let networks = rpcs::Entity::find().all(&self.db).await.map_err(|err| {
            SvmIndexerError::db_error("Failed loading rpcs for commitment sweep!", err)
        })?;

        for rpc in networks {
            let program_ids: Vec<i32> = programs::Entity::find()
                .filter(programs::Column::RpcId.eq(rpc.id))
                .all(&self.db)
                .await
                .map_err(|err| SvmIndexerError::db_error("Failed loading rpc programs!", err))?
                .into_iter()
                .map(|program| program.id)
                .collect();
            if program_ids.is_empty() {
                continue;
            }

            let client = self.client_for(&rpc).await;
            let head = match client.get_slot().await {
                Ok(head) => head as i64,
                Err(err) => {
                    tracing::warn!(rpc_id = rpc.id, error = %err, "Failed reading head for commitment sweep!");
                    continue;
                }
            };
            let floor = (head - COMMITMENT_RECENT_SLOTS).max(0);

            let pending = transactions::Entity::find()
                .filter(transactions::Column::ProgramId.is_in(program_ids))
                .filter(transactions::Column::Commitment.is_in([
                    TransactionCommitment::Processed.as_str(),
                    TransactionCommitment::Confirmed.as_str(),
                ]))
                .filter(transactions::Column::Slot.gte(floor))
                .order_by_desc(transactions::Column::Slot)
                .limit(STATUS_BATCH)
                .all(&self.db)
                .await
                .map_err(|err| {
                    SvmIndexerError::db_error("Failed loading pending commitments!", err)
                })?;

            let mut sigs = Vec::with_capacity(pending.len());
            let mut sig_bytes = Vec::with_capacity(pending.len());
            for tx in &pending {
                if let Ok(sig) = Signature::try_from(tx.signature.as_slice()) {
                    sigs.push(sig);
                    sig_bytes.push(tx.signature.clone());
                }
            }
            if sigs.is_empty() {
                continue;
            }

            let statuses = client.get_signature_statuses(&sigs).await?;
            let mut finalized = Vec::new();
            let mut dropped = Vec::new();
            for (bytes, status) in sig_bytes.into_iter().zip(statuses) {
                match status {
                    Some(TransactionCommitment::Finalized) => finalized.push(bytes),
                    Some(_) => {} // still processed/confirmed — re-checked next tick
                    None => dropped.push(bytes), // recent yet absent from the cache => reorged away
                }
            }

            self.set_commitment(finalized, TransactionCommitment::Finalized).await?;
            self.set_commitment(dropped, TransactionCommitment::Dropped).await?;
        }
        Ok(())
    }

    async fn set_commitment(
        &self,
        signatures: Vec<Vec<u8>>,
        commitment: TransactionCommitment,
    ) -> Result<(), SvmIndexerError> {
        if signatures.is_empty() {
            return Ok(());
        }
        let count = signatures.len();
        transactions::Entity::update_many()
            .col_expr(transactions::Column::Commitment, Expr::value(commitment.as_str()))
            .filter(transactions::Column::Signature.is_in(signatures))
            .exec(&self.db)
            .await
            .map_err(|err| {
                SvmIndexerError::db_error("Failed updating transaction commitment!", err)
            })?;
        tracing::info!(count, commitment = commitment.as_str(), "advanced transaction commitment");
        Ok(())
    }

    async fn backfill_block_times_once(&self) -> Result<(), SvmIndexerError> {
        let networks = rpcs::Entity::find().all(&self.db).await.map_err(|err| {
            SvmIndexerError::db_error("Failed loading rpcs for block_time backfill!", err)
        })?;

        for rpc in networks {
            let pending = slots::Entity::find()
                .filter(slots::Column::RpcId.eq(rpc.id))
                .filter(slots::Column::BlockTime.is_null())
                .order_by_asc(slots::Column::Slot)
                .limit(BACKFILL_BATCH)
                .all(&self.db)
                .await
                .map_err(|err| {
                    SvmIndexerError::db_error("Failed loading slots missing block_time!", err)
                })?;
            if pending.is_empty() {
                continue;
            }

            let client = self.client_for(&rpc).await;
            for slot in pending {
                match client.get_block_time(slot.slot as u64).await {
                    Ok(Some(block_time)) => {
                        ensure_slot(&self.db, rpc.id, slot.slot, Some(block_time)).await?;
                    }
                    Ok(None) => {} // not available yet — retry next tick
                    Err(err) => tracing::warn!(
                        rpc_id = rpc.id,
                        slot = slot.slot,
                        error = %err,
                        "Failed fetching block_time for slot!"
                    ),
                }
            }
        }
        Ok(())
    }
}
