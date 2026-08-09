use std::sync::Arc;
use std::time::Duration;

use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};

use crate::core::error::SvmIndexerError;
use crate::db::entities::{rpcs, slots};
use crate::indexer::Indexer;
use crate::indexer::persist::ensure_slot;

const BACKFILL_BATCH: u64 = 500;

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
