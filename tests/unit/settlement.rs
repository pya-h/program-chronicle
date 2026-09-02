use crate::core::commitment::TransactionCommitment;
use crate::indexer::rpc::SignatureStatus;
use crate::indexer::settlement::{SettlementRoute, route_status};

const STORED_SLOT: i64 = 500;

fn answered(commitment: TransactionCommitment, slot: u64) -> Option<SignatureStatus> {
    Some(SignatureStatus { commitment, slot })
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
