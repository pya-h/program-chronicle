use solana_sdk::commitment_config::CommitmentConfig;
use solana_transaction_status::TransactionConfirmationStatus;

use crate::core::commitment::TransactionCommitment;

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
