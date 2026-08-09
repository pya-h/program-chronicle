pub mod json_rpc;

use std::pin::Pin;

use futures::Stream;
use serde::{Deserialize, Serialize};
use solana_sdk::{pubkey::Pubkey, signature::Signature};

use crate::core::error::SvmIndexerError;

pub type RpcResult<T> = Result<T, SvmIndexerError>;

pub type SubStream<T> = Pin<Box<dyn Stream<Item = RpcResult<T>> + Send>>;

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

    async fn get_program_accounts(
        &self,
        program: &Pubkey,
        filter: Option<&AccountFilter>,
    ) -> RpcResult<Vec<AccountSnapshot>>;

    /// Snapshots an explicit set of accounts — the address-allowlist short-circuit that avoids
    /// scanning every account a (shared) program owns. Missing accounts are simply absent.
    async fn get_multiple_accounts(
        &self,
        addresses: &[Pubkey],
    ) -> RpcResult<Vec<AccountSnapshot>>;

    /// Pages newest→oldest; `before` is exclusive.
    async fn get_signatures_for_address(
        &self,
        address: &Pubkey,
        before: Option<Signature>,
        limit: usize,
    ) -> RpcResult<Vec<SignatureInfo>>;

    async fn get_transaction(&self, signature: &Signature) -> RpcResult<RawTransaction>;

    async fn subscribe_program(
        &self,
        program: &Pubkey,
        filter: Option<&AccountFilter>,
    ) -> RpcResult<SubStream<AccountSnapshot>>;

    /// Live updates for an explicit set of accounts — the streaming half of the address-allowlist
    /// short-circuit. Merges one `accountSubscribe` per key into a single stream.
    async fn subscribe_accounts(
        &self,
        addresses: &[Pubkey],
    ) -> RpcResult<SubStream<AccountSnapshot>>;

    async fn subscribe_logs(&self, program: &Pubkey) -> RpcResult<SubStream<LogsNotification>>;
}
