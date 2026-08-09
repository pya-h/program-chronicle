use std::sync::Arc;
use std::time::Duration;

use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect,
};

use crate::core::error::SvmIndexerError;
use crate::db::entities::{rpcs, slots};
use crate::indexer::Indexer;
use crate::indexer::persist::ensure_slot;

const BACKFILL_BATCH: u64 = 500;
/// Cap the work per tx_signature backfill pass; leftovers are picked up on the next tick.
const TX_SIGNATURE_BACKFILL_MAX_BATCHES: usize = 40;

/// Attributes each `account_states` row to the transaction that wrote it, but only when a single
/// writable transaction touched that account at that slot — an ambiguous slot is left NULL. Runs as
/// batched set-based `UPDATE`s so a large backlog drains over several statements instead of one lock.
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

    /// Fills `account_states.tx_signature` left NULL by json-rpc ingest, using `transaction_accounts`.
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
