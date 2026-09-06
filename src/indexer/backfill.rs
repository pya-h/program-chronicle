use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::core::error::SvmIndexerError;
use crate::db::entities::{rpcs, slots};
use crate::indexer::Indexer;
use crate::indexer::persist::ensure_slot;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect,
};

const BACKFILL_BATCH: u64 = 500;
/// Cap the work per tx_signature backfill pass; leftovers are picked up on the next tick.
const TX_SIGNATURE_BACKFILL_MAX_BATCHES: usize = 40;

pub async fn backfill_latest_tx_signatures_once(
    db: &DatabaseConnection,
) -> Result<u64, SvmIndexerError> {
    const SQL: &str = "\
        UPDATE program_accounts pa \
        SET latest_tx_signature = m.sig, updated_at = CURRENT_TIMESTAMP \
        FROM ( \
            SELECT pa.id AS program_account_id, \
                   (ARRAY_AGG(DISTINCT ta.tx_signature))[1] AS sig \
            FROM program_accounts pa \
            JOIN transaction_accounts ta \
              ON ta.account_address = pa.address AND ta.slot = pa.latest_slot \
            WHERE ta.writable \
              AND pa.latest_tx_signature IS NULL \
            GROUP BY pa.id \
            HAVING COUNT(DISTINCT ta.tx_signature) = 1 \
            LIMIT 500 \
        ) m \
        WHERE pa.id = m.program_account_id \
          AND pa.latest_tx_signature IS NULL";

    let mut filled = 0u64;
    for _ in 0..TX_SIGNATURE_BACKFILL_MAX_BATCHES {
        let result = db.execute_unprepared(SQL).await.map_err(|err| {
            SvmIndexerError::db_error(
                "Failed backfilling program_accounts.latest_tx_signature!",
                err,
            )
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
        // per-rpc low-water slot the next scan resumes from, so a front of permanently unresolvable
        // slots can't pin the window and starve newer resolvable ones
        let mut cursors: HashMap<i32, i64> = HashMap::new();
        loop {
            ticker.tick().await;
            if let Err(err) = self.backfill_block_times_once(&mut cursors).await {
                tracing::error!(error = %err, "block_time backfill pass failed!");
            }
        }
    }

    pub async fn run_tx_signature_backfill(self: Arc<Self>, interval: Duration) {
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            match backfill_latest_tx_signatures_once(&self.db).await {
                Ok(filled) if filled > 0 => {
                    tracing::info!(filled, "tx_signature backfill attributed latest state")
                }
                Ok(_) => {}
                Err(err) => {
                    tracing::error!(error = %err, "latest_tx_signature backfill pass failed!")
                }
            }
        }
    }

    async fn backfill_block_times_once(
        &self,
        cursors: &mut HashMap<i32, i64>,
    ) -> Result<(), SvmIndexerError> {
        let networks = rpcs::Entity::find().all(&self.db).await.map_err(|err| {
            SvmIndexerError::db_error("Failed loading rpcs for block_time backfill!", err)
        })?;

        for rpc in networks {
            let from = cursors.get(&rpc.id).copied().unwrap_or(0);
            let pending = slots::Entity::find()
                .filter(slots::Column::RpcId.eq(rpc.id))
                .filter(slots::Column::BlockTime.is_null())
                .filter(slots::Column::Slot.gte(from))
                .order_by_asc(slots::Column::Slot)
                .limit(BACKFILL_BATCH)
                .all(&self.db)
                .await
                .map_err(|err| {
                    SvmIndexerError::db_error("Failed loading slots missing block_time!", err)
                })?;
            if pending.is_empty() {
                cursors.remove(&rpc.id); // window exhausted from `from`; wrap to the start next tick
                continue;
            }
            cursors.insert(rpc.id, pending.last().map_or(from, |slot| slot.slot + 1));

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
