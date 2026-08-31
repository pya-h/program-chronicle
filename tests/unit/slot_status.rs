use crate::core::slot_status::SlotStatus;

const ALL: [SlotStatus; 4] = [
    SlotStatus::Unknown,
    SlotStatus::Canonical,
    SlotStatus::Reorged,
    SlotStatus::Pruned,
];

#[test]
fn as_str_round_trips_every_status() {
    for status in ALL {
        assert_eq!(SlotStatus::from_query(status.as_str()), Some(status));
    }
}

#[test]
fn from_query_accepts_known_statuses_only() {
    assert_eq!(
        SlotStatus::from_query("canonical"),
        Some(SlotStatus::Canonical)
    );
    assert_eq!(SlotStatus::from_query("Reorged"), None);
    assert_eq!(SlotStatus::from_query(""), None);
    // the node's own word for -32007, deliberately not a status: mapping it here would read a
    // reorg as a slot that never had a block
    assert_eq!(SlotStatus::from_query("skipped"), None);
}

#[test]
fn only_a_chain_verdict_settles_a_slot() {
    assert!(SlotStatus::Canonical.is_settled());
    assert!(SlotStatus::Reorged.is_settled());
    // both mean "no answer yet" — a settled slot is never re-checked, so these must stay open
    assert!(!SlotStatus::Unknown.is_settled());
    assert!(!SlotStatus::Pruned.is_settled());
}
