use solana_sdk::commitment_config::CommitmentConfig;
use solana_transaction_status::TransactionConfirmationStatus;

use crate::core::commitment::TransactionCommitment;
use crate::indexer::backfill::classify_commitment_sweep;

#[test]
fn as_str_round_trips_every_level() {
    assert_eq!(TransactionCommitment::Processed.as_str(), "processed");
    assert_eq!(TransactionCommitment::Confirmed.as_str(), "confirmed");
    assert_eq!(TransactionCommitment::Finalized.as_str(), "finalized");
    assert_eq!(TransactionCommitment::Dropped.as_str(), "dropped");
}

#[test]
fn from_ingest_only_finalized_starts_final() {
    assert_eq!(
        TransactionCommitment::from_ingest(CommitmentConfig::finalized()),
        TransactionCommitment::Finalized
    );
    // processed ingest still enters at confirmed — the worker promotes it from there
    assert_eq!(
        TransactionCommitment::from_ingest(CommitmentConfig::confirmed()),
        TransactionCommitment::Confirmed
    );
    assert_eq!(
        TransactionCommitment::from_ingest(CommitmentConfig::processed()),
        TransactionCommitment::Confirmed
    );
}

#[test]
fn from_query_accepts_known_levels_only() {
    assert_eq!(
        TransactionCommitment::from_query("processed"),
        Some(TransactionCommitment::Processed)
    );
    assert_eq!(
        TransactionCommitment::from_query("confirmed"),
        Some(TransactionCommitment::Confirmed)
    );
    assert_eq!(
        TransactionCommitment::from_query("finalized"),
        Some(TransactionCommitment::Finalized)
    );
    assert_eq!(
        TransactionCommitment::from_query("dropped"),
        Some(TransactionCommitment::Dropped)
    );
    // typos and casing are rejected rather than silently matching nothing
    assert_eq!(TransactionCommitment::from_query("finalizd"), None);
    assert_eq!(TransactionCommitment::from_query("Confirmed"), None);
    assert_eq!(TransactionCommitment::from_query(""), None);
}

#[test]
fn commitment_sweep_promotes_finalized_and_drops_absent() {
    let sig = |n: u8| vec![n; 4];
    let signatures = vec![sig(1), sig(2), sig(3), sig(4)];
    let statuses = vec![
        Some(TransactionCommitment::Finalized), // promote
        Some(TransactionCommitment::Confirmed), // still pending, untouched
        Some(TransactionCommitment::Processed), // still pending, untouched
        None,                                   // absent from recent cache => reorged away
    ];

    let (finalized, dropped) = classify_commitment_sweep(signatures, statuses);
    assert_eq!(finalized, vec![sig(1)]);
    assert_eq!(dropped, vec![sig(4)]);
}

#[test]
fn commitment_sweep_partitions_are_empty_when_all_still_pending() {
    let signatures = vec![vec![1; 4], vec![2; 4]];
    let statuses = vec![
        Some(TransactionCommitment::Confirmed),
        Some(TransactionCommitment::Processed),
    ];
    let (finalized, dropped) = classify_commitment_sweep(signatures, statuses);
    assert!(finalized.is_empty());
    assert!(dropped.is_empty());
}

#[test]
fn maps_node_confirmation_status() {
    assert_eq!(
        TransactionCommitment::from(&TransactionConfirmationStatus::Processed),
        TransactionCommitment::Processed
    );
    assert_eq!(
        TransactionCommitment::from(&TransactionConfirmationStatus::Confirmed),
        TransactionCommitment::Confirmed
    );
    assert_eq!(
        TransactionCommitment::from(&TransactionConfirmationStatus::Finalized),
        TransactionCommitment::Finalized
    );
}
