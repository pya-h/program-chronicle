use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use sea_orm::{
    ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    Statement, TransactionTrait, Value, sea_query::Expr,
};
use solana_sdk::signature::Signature;

use crate::core::commitment::TransactionCommitment;
use crate::core::error::SvmIndexerError;
use crate::core::slot_status::SlotStatus;
use crate::db::entities::{
    program_events, programs, rpcs, slots, transaction_accounts, transactions,
};
use crate::indexer::Indexer;
use crate::indexer::persist::ensure_slot;
use crate::indexer::rpc::SignatureStatus;

/// `getSignatureStatuses` caps at 256 signatures per call.
const STATUS_CHUNK: usize = 256;

/// What the node's status answer says about one stored row. `Absent` is the only route that can ever
/// lead to a drop, and never on its own — the block read is what turns it into evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SettlementRoute {
    Finalized,
    Survivor(i64),
    Pending,
    Absent,
}

/// Kept apart from the batching around it: the escalation pass runs the same four ways over the rows it could not reach.
pub(crate) fn route_status(stored_slot: i64, status: Option<SignatureStatus>) -> SettlementRoute {
    match status {
        Some(status) if status.commitment == TransactionCommitment::Finalized => {
            let slot = status.slot as i64;
            if slot == stored_slot {
                SettlementRoute::Finalized
            } else {
                SettlementRoute::Survivor(slot)
            }
        }
        Some(_) => SettlementRoute::Pending,
        None => SettlementRoute::Absent,
    }
}

impl Indexer {
    /// Settles every unsettled transaction, with no recency window: the candidate set is what we
    /// hold, not what is near the head, which is what keeps a row from sliding out of reach.
    pub async fn run_settlement_worker(self: Arc<Self>, interval: Duration, batch: u64) {
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            if let Err(err) = self.settle_once(batch).await {
                tracing::error!(error = %err, "settlement pass failed!");
            }
        }
    }

    async fn settle_once(&self, batch: u64) -> Result<(), SvmIndexerError> {
        let networks = rpcs::Entity::find()
            .all(&self.db)
            .await
            .map_err(|err| SvmIndexerError::db_error("Failed loading rpcs for settlement!", err))?;

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

            let pending = transactions::Entity::find()
                .filter(transactions::Column::ProgramId.is_in(program_ids))
                .filter(transactions::Column::Commitment.is_in([
                    TransactionCommitment::Processed.as_str(),
                    TransactionCommitment::Confirmed.as_str(),
                ]))
                // oldest first under a single cap, so the cut lands inside one slot per tick — a slot
                // is never left half-answered from both ends
                .order_by_asc(transactions::Column::Slot)
                .limit(batch)
                .all(&self.db)
                .await
                .map_err(|err| {
                    SvmIndexerError::db_error("Failed loading unsettled transactions!", err)
                })?;
            if pending.is_empty() {
                continue;
            }

            let client = self.client_for(&rpc).await;
            let mut finalized: Vec<Vec<u8>> = Vec::new();
            let mut survivors: Vec<(Vec<u8>, i64)> = Vec::new();
            let mut canonical: HashSet<i64> = HashSet::new();

            for chunk in pending.chunks(STATUS_CHUNK) {
                let mut rows = Vec::with_capacity(chunk.len());
                let mut signatures = Vec::with_capacity(chunk.len());
                for tx in chunk {
                    if let Ok(signature) = Signature::try_from(tx.signature.as_slice()) {
                        signatures.push(signature);
                        rows.push(tx);
                    }
                }
                if signatures.is_empty() {
                    continue;
                }

                let statuses = match client
                    .get_signature_statuses_with_history(&signatures)
                    .await
                {
                    Ok(statuses) => statuses,
                    Err(err) => {
                        // inconclusive: the rows keep the level they have and the next tick asks again
                        tracing::warn!(rpc_id = rpc.id, error = %err, "Failed reading signature statuses for settlement!");
                        break;
                    }
                };

                for (tx, status) in rows.into_iter().zip(statuses) {
                    match route_status(tx.slot, status) {
                        SettlementRoute::Finalized => {
                            finalized.push(tx.signature.clone());
                            canonical.insert(tx.slot);
                        }
                        SettlementRoute::Survivor(slot) => {
                            survivors.push((tx.signature.clone(), slot));
                            canonical.insert(slot);
                        }
                        // the escalation pass is what turns an absence into a verdict
                        SettlementRoute::Pending | SettlementRoute::Absent => {}
                    }
                }
            }

            for (signature, slot) in survivors {
                if let Err(err) = self.reslot_survivor(rpc.id, &signature, slot).await {
                    tracing::warn!(rpc_id = rpc.id, slot, error = %err, "Failed re-slotting a re-included transaction!");
                }
            }
            if let Err(err) = self
                .set_commitment(finalized, TransactionCommitment::Finalized)
                .await
            {
                tracing::warn!(rpc_id = rpc.id, error = %err, "Failed promoting settled transactions!");
                continue;
            }
            if let Err(err) = self.mark_canonical(rpc.id, canonical).await {
                tracing::warn!(rpc_id = rpc.id, error = %err, "Failed marking slots canonical!");
            }
        }
        Ok(())
    }

    pub(crate) async fn set_commitment(
        &self,
        signatures: Vec<Vec<u8>>,
        commitment: TransactionCommitment,
    ) -> Result<(), SvmIndexerError> {
        if signatures.is_empty() {
            return Ok(());
        }
        let count = signatures.len();
        transactions::Entity::update_many()
            .col_expr(
                transactions::Column::Commitment,
                Expr::value(commitment.as_str()),
            )
            .filter(transactions::Column::Signature.is_in(signatures))
            .exec(&self.db)
            .await
            .map_err(|err| {
                SvmIndexerError::db_error("Failed updating transaction commitment!", err)
            })?;
        tracing::info!(
            count,
            commitment = commitment.as_str(),
            "advanced transaction commitment"
        );
        Ok(())
    }

    async fn mark_canonical(
        &self,
        rpc_id: i32,
        observed: HashSet<i64>,
    ) -> Result<(), SvmIndexerError> {
        if observed.is_empty() {
            return Ok(());
        }
        slots::Entity::update_many()
            .col_expr(
                slots::Column::Status,
                Expr::value(SlotStatus::Canonical.as_str()),
            )
            .col_expr(
                slots::Column::UpdatedAt,
                Expr::value(chrono::Utc::now().naive_utc()),
            )
            .filter(slots::Column::RpcId.eq(rpc_id))
            .filter(slots::Column::Slot.is_in(observed))
            // never over-write a verdict the escalation already reached
            .filter(slots::Column::Status.eq(SlotStatus::Unknown.as_str()))
            .exec(&self.db)
            .await
            .map_err(|err| SvmIndexerError::db_error("Failed marking slots canonical!", err))?;
        Ok(())
    }

    async fn reslot_survivor(
        &self,
        rpc_id: i32,
        signature: &[u8],
        slot: i64,
    ) -> Result<(), SvmIndexerError> {
        let txn = self.db.begin().await.map_err(|err| {
            SvmIndexerError::db_error("Failed opening the re-slot transaction!", err)
        })?;

        let slot_id = ensure_slot(&txn, rpc_id, slot, None).await?;
        transactions::Entity::update_many()
            .col_expr(transactions::Column::Slot, Expr::value(slot))
            .col_expr(transactions::Column::SlotId, Expr::value(slot_id))
            .col_expr(
                transactions::Column::Commitment,
                Expr::value(TransactionCommitment::Finalized.as_str()),
            )
            .filter(transactions::Column::Signature.eq(signature.to_vec()))
            .exec(&txn)
            .await
            .map_err(|err| SvmIndexerError::db_error("Failed re-slotting the transaction!", err))?;

        // else `?slot=` on events contradicts the transaction they came from
        program_events::Entity::update_many()
            .col_expr(program_events::Column::Slot, Expr::value(slot))
            .col_expr(program_events::Column::SlotId, Expr::value(slot_id))
            .filter(program_events::Column::TxSignature.eq(signature.to_vec()))
            .exec(&txn)
            .await
            .map_err(|err| SvmIndexerError::db_error("Failed re-slotting its events!", err))?;

        // else the address→transaction join attributes to a dead slot
        transaction_accounts::Entity::update_many()
            .col_expr(transaction_accounts::Column::Slot, Expr::value(slot))
            .filter(transaction_accounts::Column::TxSignature.eq(signature.to_vec()))
            .exec(&txn)
            .await
            .map_err(|err| {
                SvmIndexerError::db_error("Failed re-slotting its account touches!", err)
            })?;

        // Keyed on the signature, never on `latest_slot` — that can hit an account read's context
        // slot by coincidence — and scoped by `rpc_id`, since a `slots` id is only valid for its own
        // network. Moving `latest_slot` backwards is a correction here, so `persist`'s monotonic
        // guard against stale writes must not apply.
        txn.execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "UPDATE program_accounts pa \
             SET latest_slot = $1, latest_slot_id = $2, updated_at = CURRENT_TIMESTAMP \
             FROM programs p \
             WHERE pa.program_id = p.id AND p.rpc_id = $3 AND pa.latest_tx_signature = $4",
            [
                Value::from(slot),
                Value::from(slot_id),
                Value::from(rpc_id),
                Value::from(signature.to_vec()),
            ],
        ))
        .await
        .map_err(|err| SvmIndexerError::db_error("Failed re-slotting its account state!", err))?;

        txn.commit()
            .await
            .map_err(|err| SvmIndexerError::db_error("Failed committing the re-slot!", err))?;
        tracing::info!(
            slot,
            "re-slotted a transaction re-included on the winning fork"
        );
        Ok(())
    }
}
