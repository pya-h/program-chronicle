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
fn only_a_missing_block_answers_without_a_download() {
    // no block at all, so the rows evidenced absent are dropped; below the floor, they all finalize
    assert!(SlotStatus::Reorged.answers_without_block());
    assert!(SlotStatus::Pruned.answers_without_block());

    assert!(!SlotStatus::Unknown.answers_without_block());
    // a transaction indexed at a canonical slot later still needs the signature list, so marking
    // the slot verified must never skip the read
    assert!(!SlotStatus::Canonical.answers_without_block());
}
