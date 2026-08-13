use solana_sdk::commitment_config::CommitmentConfig;
use solana_transaction_status::TransactionConfirmationStatus;

/// The commitment lifecycle of a stored transaction, orthogonal to its `success`/`err` result.
/// `Dropped` is our own designation for a row whose block was reorged away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransactionCommitment {
    Processed,
    Confirmed,
    Finalized,
    Dropped,
}

impl TransactionCommitment {
    pub fn as_str(self) -> &'static str {
        match self {
            TransactionCommitment::Processed => "processed",
            TransactionCommitment::Confirmed => "confirmed",
            TransactionCommitment::Finalized => "finalized",
            TransactionCommitment::Dropped => "dropped",
        }
    }

    /// Parses a `?commitment=` query value, so an unknown level is rejected instead of silently
    /// matching nothing.
    pub fn from_query(value: &str) -> Option<Self> {
        match value {
            "processed" => Some(TransactionCommitment::Processed),
            "confirmed" => Some(TransactionCommitment::Confirmed),
            "finalized" => Some(TransactionCommitment::Finalized),
            "dropped" => Some(TransactionCommitment::Dropped),
            _ => None,
        }
    }

    /// The level a freshly ingested row starts at, from the indexer's ingest commitment.
    pub fn from_ingest(commitment: CommitmentConfig) -> Self {
        if commitment == CommitmentConfig::finalized() {
            TransactionCommitment::Finalized
        } else {
            TransactionCommitment::Confirmed
        }
    }
}

impl From<&TransactionConfirmationStatus> for TransactionCommitment {
    fn from(status: &TransactionConfirmationStatus) -> Self {
        match status {
            TransactionConfirmationStatus::Processed => TransactionCommitment::Processed,
            TransactionConfirmationStatus::Confirmed => TransactionCommitment::Confirmed,
            TransactionConfirmationStatus::Finalized => TransactionCommitment::Finalized,
        }
    }
}
