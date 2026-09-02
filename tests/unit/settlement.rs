use std::collections::HashSet;

use solana_sdk::signature::Signature;

use crate::core::commitment::TransactionCommitment;
use crate::core::slot_status::SlotStatus;
use crate::indexer::rpc::{BlockSignatures, SignatureStatus, SlotBlock};
use crate::indexer::settlement::{SettlementRoute, route_status, split_by_block};

const STORED_SLOT: i64 = 500;

fn answered(commitment: TransactionCommitment, slot: u64) -> Option<SignatureStatus> {
    Some(SignatureStatus { commitment, slot })
}

/// Distinct, stable signature bytes; the values themselves carry no meaning.
fn signature(seed: u8) -> Vec<u8> {
    vec![seed; 64]
}

fn block_holding(seeds: &[u8]) -> SlotBlock {
    SlotBlock::Found(BlockSignatures {
        blockhash: "whatever".to_owned(),
        block_time: None,
        signatures: seeds
            .iter()
            .map(|seed| Signature::from([*seed; 64]))
            .collect(),
    })
}

#[test]
fn a_finalized_answer_at_our_own_slot_promotes() {
    assert_eq!(
        route_status(
            STORED_SLOT,
            answered(TransactionCommitment::Finalized, STORED_SLOT as u64)
        ),
        SettlementRoute::Finalized
    );
}

#[test]
fn a_finalized_answer_elsewhere_is_a_survivor_not_a_promotion() {
    assert_eq!(
        route_status(STORED_SLOT, answered(TransactionCommitment::Finalized, 507)),
        SettlementRoute::Survivor(507)
    );
}

#[test]
fn an_unsettled_answer_waits_for_the_next_tick() {
    for commitment in [
        TransactionCommitment::Processed,
        TransactionCommitment::Confirmed,
    ] {
        assert_eq!(
            route_status(STORED_SLOT, answered(commitment, STORED_SLOT as u64)),
            SettlementRoute::Pending
        );
        // even reported elsewhere: nothing is final yet, so there is no settled slot to move it to
        assert_eq!(
            route_status(STORED_SLOT, answered(commitment, 507)),
            SettlementRoute::Pending
        );
    }
}

#[test]
fn only_a_ledger_wide_miss_can_ever_reach_a_drop() {
    assert_eq!(route_status(STORED_SLOT, None), SettlementRoute::Absent);
}

#[test]
fn a_read_block_promotes_what_it_holds_and_drops_what_pass_one_missed() {
    let verdict = split_by_block(
        &block_holding(&[1]),
        vec![signature(1), signature(2)],
        &HashSet::from([signature(2)]),
    );
    assert_eq!(verdict.promote, vec![signature(1)]);
    assert_eq!(verdict.drop, vec![signature(2)]);
    assert_eq!(verdict.status, Some(SlotStatus::Canonical));
}

#[test]
fn a_row_pass_one_never_answered_for_is_never_dropped_on_the_slots_evidence() {
    // the §8 hazard: absent from the block at S is also what a survivor at S′ looks like, and a drop
    // is terminal, so an unevidenced row goes back through the per-signature question
    for block in [block_holding(&[]), SlotBlock::Reorged] {
        let verdict = split_by_block(&block, vec![signature(3)], &HashSet::new());
        assert!(verdict.drop.is_empty());
        assert_eq!(verdict.leftovers, vec![signature(3)]);
    }
}

#[test]
fn no_block_on_the_canonical_chain_drops_only_the_evidenced_rows() {
    let verdict = split_by_block(
        &SlotBlock::Reorged,
        vec![signature(1), signature(2)],
        &HashSet::from([signature(1)]),
    );
    assert_eq!(verdict.drop, vec![signature(1)]);
    assert_eq!(verdict.leftovers, vec![signature(2)]);
    assert!(verdict.promote.is_empty());
    assert_eq!(verdict.status, Some(SlotStatus::Reorged));
}

#[test]
fn a_pruned_slot_finalizes_every_row_and_drops_none() {
    // the one verdict that takes the whole slot: the inference rests on having reached `confirmed`,
    // which holds whether or not pass 1 asked about the row
    let verdict = split_by_block(
        &SlotBlock::Pruned,
        vec![signature(1), signature(2)],
        &HashSet::from([signature(1)]),
    );
    assert_eq!(verdict.promote, vec![signature(1), signature(2)]);
    assert!(verdict.drop.is_empty());
    assert!(verdict.leftovers.is_empty());
    assert_eq!(verdict.status, Some(SlotStatus::Pruned));
}

#[test]
fn a_slot_that_has_not_finalized_yet_touches_nothing() {
    let verdict = split_by_block(
        &SlotBlock::NotYetFinal,
        vec![signature(1)],
        &HashSet::from([signature(1)]),
    );
    assert_eq!(verdict, Default::default());
    // stays `unknown`, which is what makes the next tick ask again
    assert_eq!(verdict.status, None);
}
