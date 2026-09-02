use std::collections::{HashMap, HashSet};
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
use crate::indexer::rpc::{SignatureStatus, SlotBlock, SvmRpc};

/// `getSignatureStatuses` caps at 256 signatures per call.
const STATUS_CHUNK: usize = 256;

/// A stored row, as pass 1 selects it. The full model carries the raw transaction json, which neither pass reads.
type Candidate = (Vec<u8>, i64);

/// What the node's status answer says about one stored row. `Absent` is the only route that can ever
/// lead to a drop, and never on its own — the block read is what turns it into evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SettlementRoute {
    Finalized,
    Survivor(i64),
    Pending,
    Absent,
}

/// Kept apart from the batching around it: pass 2's leftover check runs the same four ways over the
/// rows pass 1 could not reach.
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

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct SlotVerdict {
    pub promote: Vec<Vec<u8>>,
    pub drop: Vec<Vec<u8>>,
    pub leftovers: Vec<Vec<u8>>,
    pub status: Option<SlotStatus>,
}

pub(crate) fn split_by_block(
    block: &SlotBlock,
    rows: Vec<Vec<u8>>,
    evidenced: &HashSet<Vec<u8>>,
) -> SlotVerdict {
    match block {
        SlotBlock::Found(found) => {
            let present: HashSet<&[u8]> = found.signatures.iter().map(|sig| sig.as_ref()).collect();
            let mut verdict = SlotVerdict {
                status: Some(SlotStatus::Canonical),
                ..Default::default()
            };
            for signature in rows {
                if present.contains(signature.as_slice()) {
                    verdict.promote.push(signature);
                } else if evidenced.contains(&signature) {
                    verdict.drop.push(signature);
                } else {
                    verdict.leftovers.push(signature);
                }
            }
            verdict
        }
        SlotBlock::Reorged => {
            let mut verdict = SlotVerdict {
                status: Some(SlotStatus::Reorged),
                ..Default::default()
            };
            for signature in rows {
                if evidenced.contains(&signature) {
                    verdict.drop.push(signature);
                } else {
                    verdict.leftovers.push(signature);
                }
            }
            verdict
        }
        // below the node's floor: unanswerable by it forever, so its rows finalize by inference. The
        // inference rests on having reached `confirmed`, which holds whether or not pass 1 asked, so
        // this one verdict does take the whole slot.
        SlotBlock::Pruned => SlotVerdict {
            promote: rows,
            status: Some(SlotStatus::Pruned),
            ..Default::default()
        },
        SlotBlock::NotYetFinal => SlotVerdict::default(),
    }
}

pub(crate) async fn set_commitment<C: ConnectionTrait>(
    db: &C,
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
        .exec(db)
        .await
        .map_err(|err| SvmIndexerError::db_error("Failed updating transaction commitment!", err))?;
    tracing::info!(
        count,
        commitment = commitment.as_str(),
        "advanced transaction commitment"
    );
    Ok(())
}

impl Indexer {
    /// Settles every unsettled transaction, with no recency window: the candidate set is what we
    /// hold, not what is near the head, which is what keeps a row from sliding out of reach.
    pub async fn run_settlement_worker(
        self: Arc<Self>,
        interval: Duration,
        batch: u64,
        escalations: usize,
    ) {
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            if let Err(err) = self.settle_once(batch, escalations).await {
                tracing::error!(error = %err, "settlement pass failed!");
            }
        }
    }

    async fn settle_once(&self, batch: u64, escalations: usize) -> Result<(), SvmIndexerError> {
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

            let client = self.client_for(&rpc).await;
            let absent = self
                .settle_from_statuses(rpc.id, &program_ids, client.as_ref(), batch)
                .await?;
            if !absent.is_empty() {
                self.escalate(
                    rpc.id,
                    &program_ids,
                    client.as_ref(),
                    absent,
                    escalations.max(1),
                )
                .await?;
            }
        }
        Ok(())
    }

    /// Pass 1 — what the ledger says about each signature. Everything it can settle costs ~2 KB per
    /// 256 rows; what it cannot goes to the block read, which is the expensive half.
    async fn settle_from_statuses(
        &self,
        rpc_id: i32,
        program_ids: &[i32],
        client: &dyn SvmRpc,
        batch: u64,
    ) -> Result<HashMap<i64, Vec<Vec<u8>>>, SvmIndexerError> {
        let pending: Vec<Candidate> = transactions::Entity::find()
            .select_only()
            .column(transactions::Column::Signature)
            .column(transactions::Column::Slot)
            .filter(transactions::Column::ProgramId.is_in(program_ids.to_vec()))
            .filter(transactions::Column::Commitment.is_in([
                TransactionCommitment::Processed.as_str(),
                TransactionCommitment::Confirmed.as_str(),
            ]))
            // oldest first under a single cap, so the cut lands inside one slot per tick — a slot is
            // never left half-answered from both ends
            .order_by_asc(transactions::Column::Slot)
            .limit(batch)
            .into_tuple()
            .all(&self.db)
            .await
            .map_err(|err| {
                SvmIndexerError::db_error("Failed loading unsettled transactions!", err)
            })?;

        let mut finalized: Vec<Vec<u8>> = Vec::new();
        let mut survivors: Vec<(Vec<u8>, i64)> = Vec::new();
        let mut canonical: HashSet<i64> = HashSet::new();
        let mut absent: HashMap<i64, Vec<Vec<u8>>> = HashMap::new();

        for chunk in pending.chunks(STATUS_CHUNK) {
            let Some(statuses) = self.read_statuses(rpc_id, client, chunk).await else {
                break;
            };
            for ((signature, slot), status) in chunk.iter().zip(statuses) {
                match route_status(*slot, status) {
                    SettlementRoute::Finalized => {
                        finalized.push(signature.clone());
                        canonical.insert(*slot);
                    }
                    SettlementRoute::Survivor(moved) => {
                        survivors.push((signature.clone(), moved));
                        canonical.insert(moved);
                    }
                    SettlementRoute::Absent => {
                        absent.entry(*slot).or_default().push(signature.clone())
                    }
                    SettlementRoute::Pending => {}
                }
            }
        }

        for (signature, slot) in survivors {
            if let Err(err) = self.reslot_survivor(rpc_id, &signature, slot).await {
                tracing::warn!(rpc_id, slot, error = %err, "Failed re-slotting a re-included transaction!");
            }
        }
        set_commitment(&self.db, finalized, TransactionCommitment::Finalized).await?;
        self.mark_canonical(rpc_id, canonical).await?;
        Ok(absent)
    }

    /// Pass 2 — the slots holding pass-1 absentees, and only those. Each is one block read, so the
    /// verdict is bought once per slot rather than once per row.
    async fn escalate(
        &self,
        rpc_id: i32,
        program_ids: &[i32],
        client: &dyn SvmRpc,
        mut absent: HashMap<i64, Vec<Vec<u8>>>,
        budget: usize,
    ) -> Result<(), SvmIndexerError> {
        let mut suspected: Vec<i64> = absent.keys().copied().collect();
        suspected.sort_unstable();
        if suspected.len() > budget {
            let deferred = suspected.len() - budget;
            suspected.truncate(budget);
            tracing::info!(
                rpc_id,
                deferred,
                "held slot escalations to the per-tick budget"
            );
        }

        let verdicts: HashMap<i64, (i64, SlotStatus)> = slots::Entity::find()
            .select_only()
            .column(slots::Column::Slot)
            .column(slots::Column::Id)
            .column(slots::Column::Status)
            .filter(slots::Column::RpcId.eq(rpc_id))
            .filter(slots::Column::Slot.is_in(suspected.clone()))
            .into_tuple::<(i64, i64, String)>()
            .all(&self.db)
            .await
            .map_err(|err| SvmIndexerError::db_error("Failed loading slot verdicts!", err))?
            .into_iter()
            .map(|(slot, id, status)| {
                let status = SlotStatus::from_query(&status).unwrap_or(SlotStatus::Unknown);
                (slot, (id, status))
            })
            .collect();

        // a superset of the absentees: pass 1's cap can cut through a slot, and a row inserted since
        // it ran is unasked by construction
        let held: Vec<Candidate> = transactions::Entity::find()
            .select_only()
            .column(transactions::Column::Signature)
            .column(transactions::Column::Slot)
            .filter(transactions::Column::ProgramId.is_in(program_ids.to_vec()))
            .filter(transactions::Column::Commitment.is_in([
                TransactionCommitment::Processed.as_str(),
                TransactionCommitment::Confirmed.as_str(),
            ]))
            .filter(transactions::Column::Slot.is_in(suspected.clone()))
            .into_tuple()
            .all(&self.db)
            .await
            .map_err(|err| SvmIndexerError::db_error("Failed loading a slot's rows!", err))?;
        let mut rows_at: HashMap<i64, Vec<Vec<u8>>> = HashMap::new();
        for (signature, slot) in held {
            rows_at.entry(slot).or_default().push(signature);
        }

        for slot in suspected {
            let rows = rows_at.remove(&slot).unwrap_or_default();
            let evidenced: HashSet<Vec<u8>> = absent
                .remove(&slot)
                .unwrap_or_default()
                .into_iter()
                .collect();
            let Some(&(slot_id, status)) = verdicts.get(&slot) else {
                continue;
            };
            if rows.is_empty() {
                continue;
            }

            // a verdict reachable without the block is applied straight from the ledger; without this
            // a reorged slot is re-downloaded on every tick its remaining rows sit unsettled
            let block = if status.answers_without_block() {
                if status == SlotStatus::Reorged {
                    SlotBlock::Reorged
                } else {
                    SlotBlock::Pruned
                }
            } else {
                match client.get_block_signatures(slot as u64).await {
                    Ok(block) => block,
                    Err(err) => {
                        // the node could not answer, which says nothing about the chain
                        tracing::warn!(rpc_id, slot, error = %err, "Failed reading a suspected slot's block!");
                        continue;
                    }
                }
            };

            let mut split = split_by_block(&block, rows, &evidenced);
            let Some(verdict) = split.status else {
                continue; // not finalized yet: the slot stays `unknown` and the next tick asks again
            };

            let survivors = self
                .check_leftovers(rpc_id, client, slot, split.leftovers, &mut split.drop)
                .await;

            let txn = self.db.begin().await.map_err(|err| {
                SvmIndexerError::db_error("Failed opening the settlement transaction!", err)
            })?;
            set_commitment(&txn, split.promote, TransactionCommitment::Finalized).await?;
            set_commitment(&txn, split.drop, TransactionCommitment::Dropped).await?;
            slots::Entity::update_many()
                .col_expr(slots::Column::Status, Expr::value(verdict.as_str()))
                .col_expr(
                    slots::Column::UpdatedAt,
                    Expr::value(chrono::Utc::now().naive_utc()),
                )
                .filter(slots::Column::Id.eq(slot_id))
                .exec(&txn)
                .await
                .map_err(|err| {
                    SvmIndexerError::db_error("Failed recording a slot verdict!", err)
                })?;
            txn.commit().await.map_err(|err| {
                SvmIndexerError::db_error("Failed committing the settlement!", err)
            })?;

            for (signature, moved) in survivors {
                if let Err(err) = self.reslot_survivor(rpc_id, &signature, moved).await {
                    tracing::warn!(rpc_id, slot = moved, error = %err, "Failed re-slotting a re-included transaction!");
                }
            }
        }
        Ok(())
    }

    /// Rows at a settled slot that pass 1 never answered for. Absence from the block is a slot-level
    /// fact, and dropping them on it is unrecoverable, so they get the per-signature question first.
    /// Costs ~2 KB on a slot that already paid for a block download.
    async fn check_leftovers(
        &self,
        rpc_id: i32,
        client: &dyn SvmRpc,
        slot: i64,
        leftovers: Vec<Vec<u8>>,
        drop: &mut Vec<Vec<u8>>,
    ) -> Vec<(Vec<u8>, i64)> {
        let mut survivors = Vec::new();
        // the cap can leave more of them than one call carries
        for chunk in leftovers.chunks(STATUS_CHUNK) {
            let chunk: Vec<Candidate> = chunk.iter().map(|sig| (sig.clone(), slot)).collect();
            let Some(statuses) = self.read_statuses(rpc_id, client, &chunk).await else {
                break;
            };
            for ((signature, _), status) in chunk.into_iter().zip(statuses) {
                match route_status(slot, status) {
                    SettlementRoute::Survivor(moved) => survivors.push((signature, moved)),
                    SettlementRoute::Absent => drop.push(signature),
                    // `Finalized` here contradicts the block we just read, and `Pending` is not
                    // evidence of anything: both wait rather than guess
                    SettlementRoute::Finalized | SettlementRoute::Pending => {}
                }
            }
        }
        survivors
    }

    /// `None` on failure: an unanswered call is not evidence, so the caller stops rather than
    /// treating silence as an answer.
    async fn read_statuses(
        &self,
        rpc_id: i32,
        client: &dyn SvmRpc,
        chunk: &[Candidate],
    ) -> Option<Vec<Option<SignatureStatus>>> {
        let signatures: Vec<Signature> = chunk
            .iter()
            .filter_map(|(bytes, _)| Signature::try_from(bytes.as_slice()).ok())
            .collect();
        if signatures.len() != chunk.len() {
            tracing::warn!(
                rpc_id,
                "A stored signature is unparseable; skipping the batch!"
            );
            return None;
        }
        match client
            .get_signature_statuses_with_history(&signatures)
            .await
        {
            Ok(statuses) => Some(statuses),
            Err(err) => {
                // inconclusive: the rows keep the level they have and the next tick asks again
                tracing::warn!(rpc_id, error = %err, "Failed reading signature statuses for settlement!");
                None
            }
        }
    }

    /// A finalized transaction *at* S can only be true if a finalized block exists at S, so this is
    /// what keeps a healthy slot from sitting at `unknown` forever — the escalation only ever visits
    /// suspected reorgs.
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

    /// Moves a re-included transaction to the slot the node now reports. The slot is denormalised
    /// across four tables, so a partial move would leave them contradicting the row they describe.
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
