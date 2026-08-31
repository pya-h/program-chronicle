pub mod json_rpc;

use std::pin::Pin;

use futures::Stream;
use serde::{Deserialize, Serialize};
use solana_sdk::{pubkey::Pubkey, signature::Signature};
use tokio::sync::oneshot;

use crate::core::commitment::TransactionCommitment;
use crate::core::error::SvmIndexerError;

pub type RpcResult<T> = Result<T, SvmIndexerError>;

pub type SubStream<T> = Pin<Box<dyn Stream<Item = RpcResult<T>> + Send>>;

/// Fires once the node accepted the subscription. The stream handle is returned before that, so a
/// caller that must not miss updates has to await it; a dropped sender means the subscribe failed.
pub type SubReady = oneshot::Sender<()>;

#[derive(Clone, Debug)]
pub struct AccountSnapshot {
    pub address: Pubkey,
    pub owner: Pubkey,
    pub lamports: u64,
    pub data: Vec<u8>,
    pub rent_epoch: u64,
    pub executable: bool,
    pub slot: u64,
    /// Geyser-only intra-slot ordering; `None` on plain RPC (persisted as 0).
    pub write_version: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct SignatureInfo {
    pub signature: Signature,
    pub slot: u64,
    pub err: Option<serde_json::Value>,
    pub block_time: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct BlockSignatures {
    pub blockhash: String,
    pub block_time: Option<i64>,
    /// Every signature the block contains, program-agnostic.
    pub signatures: Vec<Signature>,
}

/// Why the node returned no block, kept apart from the block itself: an absent block is a verdict,
/// and which verdict depends on the code. Anything outside these three stays an `Err`.
#[derive(Clone, Debug)]
pub enum SlotBlock {
    Found(BlockSignatures),
    /// No block at this slot on the canonical chain.
    Reorged,
    /// This node had it and discarded it — no verdict possible from here.
    Pruned,
    NotYetFinal,
}

#[derive(Clone, Copy, Debug)]
pub struct SignatureStatus {
    pub commitment: TransactionCommitment,
    /// Where the node reports the transaction *now* — a survivor re-included on the winning fork
    /// names its new slot here, so re-slotting costs no extra call.
    pub slot: u64,
}

#[derive(Clone, Debug)]
pub struct RawTransaction {
    pub signature: Signature,
    pub slot: u64,
    pub block_time: Option<i64>,
    pub success: bool,
    pub err: Option<serde_json::Value>,
    pub fee: Option<u64>,
    pub fee_payer: Option<Pubkey>,
    pub logs: Vec<String>,
    /// Every account the transaction references, with its writable flag; drives `transaction_accounts`.
    pub accounts: Vec<TxAccount>,
    pub raw: serde_json::Value,
}

#[derive(Clone, Debug)]
pub struct TxAccount {
    pub address: Pubkey,
    pub writable: bool,
}

#[derive(Clone, Debug)]
pub struct LogsNotification {
    pub signature: Signature,
    pub slot: u64,
    pub err: Option<serde_json::Value>,
    pub logs: Vec<String>,
}

/// Mirrors the `getProgramAccounts` predicates so the RPC narrows server-side. `None` => track all
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AccountFilter {
    pub data_size: Option<u64>,
    #[serde(default)]
    pub memcmp: Vec<MemcmpFilter>,
    #[serde(default)]
    pub addresses: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemcmpFilter {
    pub offset: usize,
    pub bytes: String, // base58
}

#[async_trait::async_trait]
pub trait SvmRpc: Send + Sync {
    async fn get_slot(&self) -> RpcResult<u64>;

    async fn get_block_time(&self, slot: u64) -> RpcResult<Option<i64>>;

    /// The finalized block at `slot`, signatures only. A missing block is not an `Err`: the node
    /// names the cause by error code, and `SlotBlock` carries it through.
    async fn get_block_signatures(&self, slot: u64) -> RpcResult<SlotBlock>;

    async fn get_program_accounts(
        &self,
        program: &Pubkey,
        filter: Option<&AccountFilter>,
    ) -> RpcResult<Vec<AccountSnapshot>>;

    /// Snapshots an explicit set of accounts — the address-allowlist short-circuit that avoids
    /// scanning every account a (shared) program owns. Missing accounts are simply absent.
    async fn get_multiple_accounts(&self, addresses: &[Pubkey]) -> RpcResult<Vec<AccountSnapshot>>;

    /// Pages newest→oldest; `before` is exclusive.
    async fn get_signatures_for_address(
        &self,
        address: &Pubkey,
        before: Option<Signature>,
        limit: usize,
    ) -> RpcResult<Vec<SignatureInfo>>;

    async fn get_transaction(&self, signature: &Signature) -> RpcResult<RawTransaction>;

    async fn get_transactions(
        &self,
        signatures: &[Signature],
    ) -> RpcResult<Vec<Option<RawTransaction>>> {
        let mut transactions = Vec::with_capacity(signatures.len());
        for signature in signatures {
            transactions.push(Some(self.get_transaction(signature).await?));
        }
        Ok(transactions)
    }

    async fn get_signature_statuses(
        &self,
        signatures: &[Signature],
    ) -> RpcResult<Vec<Option<TransactionCommitment>>>;

    async fn get_signature_statuses_with_history(
        &self,
        signatures: &[Signature],
    ) -> RpcResult<Vec<Option<SignatureStatus>>>;

    async fn subscribe_program(
        &self,
        program: &Pubkey,
        filter: Option<&AccountFilter>,
        ready: SubReady,
    ) -> RpcResult<SubStream<AccountSnapshot>>;

    /// Live updates for an explicit set of accounts — the streaming half of the address-allowlist
    /// short-circuit. Merges one `accountSubscribe` per key into a single stream.
    async fn subscribe_accounts(
        &self,
        addresses: &[Pubkey],
        ready: SubReady,
    ) -> RpcResult<SubStream<AccountSnapshot>>;

    async fn subscribe_logs(
        &self,
        program: &Pubkey,
        ready: SubReady,
    ) -> RpcResult<SubStream<LogsNotification>>;
}
