use async_trait::async_trait;
use futures::StreamExt;
use solana_account_decoder::UiAccountEncoding;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config;
use solana_client::rpc_filter::RpcFilterType;
use solana_pubsub_client::nonblocking::pubsub_client::PubsubClient;
use solana_rpc_client_api::config::{
    RpcAccountInfoConfig, RpcProgramAccountsConfig, RpcTransactionConfig, RpcTransactionLogsConfig,
    RpcTransactionLogsFilter,
};
use solana_sdk::{
    account::Account, commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Signature,
};
use solana_transaction_status::{
    EncodedConfirmedTransactionWithStatusMeta, EncodedTransaction, UiLoadedAddresses, UiMessage,
    UiTransactionEncoding,
};

use crate::core::error::SvmIndexerError;
use crate::indexer::rpc::{
    AccountFilter, AccountSnapshot, LogsNotification, RawTransaction, RpcResult, SignatureInfo,
    SubStream, SvmRpc, TxAccount,
};

pub struct JsonRpc {
    client: RpcClient,
    ws_url: String,
    commitment: CommitmentConfig,
}

impl JsonRpc {
    pub fn new(http_url: String, ws_url: String, commitment: CommitmentConfig) -> Self {
        Self {
            client: RpcClient::new_with_commitment(http_url, commitment),
            ws_url,
            commitment,
        }
    }

    async fn connect(&self) -> RpcResult<&'static PubsubClient> {
        let pubsub = PubsubClient::new(&self.ws_url)
            .await
            .map_err(|err| SvmIndexerError::SubscriptionFailed(err.to_string()))?;
        Ok(Box::leak(Box::new(pubsub)))
    }
}

#[async_trait]
impl SvmRpc for JsonRpc {
    async fn get_slot(&self) -> RpcResult<u64> {
        self.client
            .get_slot()
            .await
            .map_err(|err| SvmIndexerError::RpcRequestFailed(err.to_string()))
    }

    async fn get_block_time(&self, slot: u64) -> RpcResult<Option<i64>> {
        // best-effort: a pruned/absent block time is not a failure, just unknown yet
        Ok(self.client.get_block_time(slot).await.ok())
    }

    async fn get_program_accounts(
        &self,
        program: &Pubkey,
        filter: Option<&AccountFilter>,
    ) -> RpcResult<Vec<AccountSnapshot>> {
        let slot = self
            .client
            .get_slot()
            .await
            .map_err(|err| SvmIndexerError::RpcRequestFailed(err.to_string()))?;
        let config = RpcProgramAccountsConfig {
            filters: rpc_filters(filter),
            account_config: RpcAccountInfoConfig {
                encoding: Some(UiAccountEncoding::Base64Zstd),
                ..Default::default()
            },
            ..Default::default()
        };
        let accounts = self
            .client
            .get_program_accounts_with_config(program, config)
            .await
            .map_err(|err| SvmIndexerError::RpcRequestFailed(err.to_string()))?;

        Ok(accounts
            .into_iter()
            .map(|(address, acct)| AccountSnapshot {
                address,
                owner: acct.owner,
                lamports: acct.lamports,
                data: acct.data,
                rent_epoch: acct.rent_epoch,
                executable: acct.executable,
                slot,
                write_version: None,
            })
            .collect())
    }

    async fn get_multiple_accounts(
        &self,
        addresses: &[Pubkey],
    ) -> RpcResult<Vec<AccountSnapshot>> {
        let config = RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64Zstd),
            ..Default::default()
        };
        let response = self
            .client
            .get_multiple_accounts_with_config(addresses, config)
            .await
            .map_err(|err| SvmIndexerError::RpcRequestFailed(err.to_string()))?;
        let slot = response.context.slot;

        Ok(addresses
            .iter()
            .zip(response.value)
            .filter_map(|(address, acct)| {
                let acct = acct?; // an unindexed / closed account is simply absent
                Some(AccountSnapshot {
                    address: *address,
                    owner: acct.owner,
                    lamports: acct.lamports,
                    data: acct.data,
                    rent_epoch: acct.rent_epoch,
                    executable: acct.executable,
                    slot,
                    write_version: None,
                })
            })
            .collect())
    }

    async fn get_signatures_for_address(
        &self,
        address: &Pubkey,
        before: Option<Signature>,
        limit: usize,
    ) -> RpcResult<Vec<SignatureInfo>> {
        let config = GetConfirmedSignaturesForAddress2Config {
            before,
            until: None,
            limit: Some(limit),
            commitment: None,
        };
        let signatures = self
            .client
            .get_signatures_for_address_with_config(address, config)
            .await
            .map_err(|err| SvmIndexerError::RpcRequestFailed(err.to_string()))?;

        signatures
            .into_iter()
            .map(|s| {
                Ok(SignatureInfo {
                    signature: s
                        .signature
                        .parse()
                        .map_err(|_| SvmIndexerError::RpcRequestFailed(s.signature.clone()))?,
                    slot: s.slot,
                    err: s.err.map(|e| serde_json::to_value(e).unwrap_or_default()),
                    block_time: s.block_time,
                })
            })
            .collect()
    }

    async fn get_transaction(&self, signature: &Signature) -> RpcResult<RawTransaction> {
        let config = RpcTransactionConfig {
            encoding: Some(UiTransactionEncoding::Json),
            commitment: None,
            max_supported_transaction_version: Some(0),
        };
        let tx = self
            .client
            .get_transaction_with_config(signature, config)
            .await
            .map_err(|err| SvmIndexerError::RpcRequestFailed(err.to_string()))?;

        let meta = tx.transaction.meta.as_ref();
        let logs = meta
            .and_then(|m| Option::<Vec<String>>::from(m.log_messages.clone()))
            .unwrap_or_default();

        Ok(RawTransaction {
            signature: *signature,
            slot: tx.slot,
            block_time: tx.block_time,
            success: meta.map(|m| m.err.is_none()).unwrap_or(false),
            err: meta
                .and_then(|m| m.err.as_ref())
                .map(|e| serde_json::to_value(e).unwrap_or_default()),
            fee: meta.map(|m| m.fee),
            fee_payer: fee_payer(&tx),
            logs,
            accounts: account_keys(&tx),
            raw: serde_json::to_value(&tx)
                .map_err(|err| SvmIndexerError::SerializationError(err.to_string()))?,
        })
    }

    async fn subscribe_program(
        &self,
        program: &Pubkey,
        filter: Option<&AccountFilter>,
    ) -> RpcResult<SubStream<AccountSnapshot>> {
        let client = self.connect().await?;
        let config = RpcProgramAccountsConfig {
            filters: rpc_filters(filter),
            account_config: RpcAccountInfoConfig {
                encoding: Some(UiAccountEncoding::Base64Zstd),
                commitment: Some(self.commitment),
                ..Default::default()
            },
            ..Default::default()
        };
        let (stream, _unsub) = client
            .program_subscribe(program, Some(config))
            .await
            .map_err(|err| SvmIndexerError::SubscriptionFailed(err.to_string()))?;

        Ok(Box::pin(stream.map(|response| {
            let slot = response.context.slot;
            let keyed = response.value;
            let address = keyed
                .pubkey
                .parse()
                .map_err(|_| SvmIndexerError::SubscriptionFailed(keyed.pubkey.clone()))?;
            let account = keyed.account.decode::<Account>().ok_or_else(|| {
                SvmIndexerError::SubscriptionFailed(format!("undecodable account {}", keyed.pubkey))
            })?;
            Ok(AccountSnapshot {
                address,
                owner: account.owner,
                lamports: account.lamports,
                data: account.data,
                rent_epoch: account.rent_epoch,
                executable: account.executable,
                slot,
                write_version: None,
            })
        })))
    }

    async fn subscribe_accounts(
        &self,
        addresses: &[Pubkey],
    ) -> RpcResult<SubStream<AccountSnapshot>> {
        let client = self.connect().await?;
        let config = RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64Zstd),
            commitment: Some(self.commitment),
            ..Default::default()
        };
        let mut streams: Vec<SubStream<AccountSnapshot>> = Vec::with_capacity(addresses.len());
        for &address in addresses {
            // accountSubscribe notifications carry no pubkey, so tag each stream with its known key
            let (stream, _unsub) = client
                .account_subscribe(&address, Some(config.clone()))
                .await
                .map_err(|err| SvmIndexerError::SubscriptionFailed(err.to_string()))?;
            streams.push(Box::pin(stream.map(move |response| {
                let account = response.value.decode::<Account>().ok_or_else(|| {
                    SvmIndexerError::SubscriptionFailed(format!("undecodable account {address}"))
                })?;
                Ok(AccountSnapshot {
                    address,
                    owner: account.owner,
                    lamports: account.lamports,
                    data: account.data,
                    rent_epoch: account.rent_epoch,
                    executable: account.executable,
                    slot: response.context.slot,
                    write_version: None,
                })
            })));
        }
        Ok(Box::pin(futures::stream::select_all(streams)))
    }

    async fn subscribe_logs(&self, program: &Pubkey) -> RpcResult<SubStream<LogsNotification>> {
        let client = self.connect().await?;
        let config = RpcTransactionLogsConfig {
            commitment: Some(self.commitment),
        };
        let (stream, _unsub) = client
            .logs_subscribe(
                RpcTransactionLogsFilter::Mentions(vec![program.to_string()]),
                config,
            )
            .await
            .map_err(|err| SvmIndexerError::SubscriptionFailed(err.to_string()))?;

        Ok(Box::pin(stream.map(|response| {
            let slot = response.context.slot;
            let value = response.value;
            Ok(LogsNotification {
                signature: value
                    .signature
                    .parse()
                    .map_err(|_| SvmIndexerError::SubscriptionFailed(value.signature.clone()))?,
                slot,
                err: value.err.map(|e| serde_json::to_value(e).unwrap_or_default()),
                logs: value.logs,
            })
        })))
    }
}

/// Server-side filters for a descriptor, or `None` when nothing narrows — in which case the RPC
/// returns/streams every account the program owns.
fn rpc_filters(filter: Option<&AccountFilter>) -> Option<Vec<RpcFilterType>> {
    let filters = filter?.rpc_filters();
    (!filters.is_empty()).then_some(filters)
}

/// fee payer is always the first account key
fn fee_payer(tx: &EncodedConfirmedTransactionWithStatusMeta) -> Option<Pubkey> {
    let EncodedTransaction::Json(ui_tx) = &tx.transaction.transaction else {
        return None;
    };
    match &ui_tx.message {
        UiMessage::Raw(raw) => raw.account_keys.first()?.parse().ok(),
        UiMessage::Parsed(parsed) => parsed.account_keys.first()?.pubkey.parse().ok(),
    }
}

/// Every account the transaction touches, with its writable flag. Static keys are classified from
/// the message header; address-lookup-table keys (versioned txs) come from the meta, writable first.
fn account_keys(tx: &EncodedConfirmedTransactionWithStatusMeta) -> Vec<TxAccount> {
    let EncodedTransaction::Json(ui_tx) = &tx.transaction.transaction else {
        return Vec::new();
    };
    let UiMessage::Raw(raw) = &ui_tx.message else {
        return Vec::new();
    };

    let signers = raw.header.num_required_signatures as usize;
    let readonly_signed = raw.header.num_readonly_signed_accounts as usize;
    let readonly_unsigned = raw.header.num_readonly_unsigned_accounts as usize;
    let writable_signers = signers.saturating_sub(readonly_signed);
    let writable_unsigned = raw
        .account_keys
        .len()
        .saturating_sub(signers)
        .saturating_sub(readonly_unsigned);

    let mut accounts: Vec<TxAccount> = raw
        .account_keys
        .iter()
        .enumerate()
        .filter_map(|(index, key)| {
            let address = key.parse().ok()?;
            let writable = if index < signers {
                index < writable_signers
            } else {
                index - signers < writable_unsigned
            };
            Some(TxAccount { address, writable })
        })
        .collect();

    let loaded = tx
        .transaction
        .meta
        .as_ref()
        .and_then(|meta| Option::<UiLoadedAddresses>::from(meta.loaded_addresses.clone()));
    if let Some(loaded) = loaded {
        let writable = loaded
            .writable
            .iter()
            .filter_map(|key| Some(TxAccount { address: key.parse().ok()?, writable: true }));
        let readonly = loaded
            .readonly
            .iter()
            .filter_map(|key| Some(TxAccount { address: key.parse().ok()?, writable: false }));
        accounts.extend(writable.chain(readonly));
    }
    accounts
}
