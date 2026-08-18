use async_trait::async_trait;
use futures::StreamExt;
use solana_account_decoder::UiAccountEncoding;
use solana_client::client_error::{ClientError, ClientErrorKind};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config;
use solana_client::rpc_filter::RpcFilterType;
use solana_pubsub_client::nonblocking::pubsub_client::PubsubClient;
use solana_rpc_client_api::config::{
    RpcAccountInfoConfig, RpcProgramAccountsConfig, RpcTransactionConfig, RpcTransactionLogsConfig,
    RpcTransactionLogsFilter,
};
use solana_rpc_client_api::request::{RpcError, RpcRequest};
use solana_rpc_client_api::response::{OptionalContext, RpcKeyedAccount};
use solana_sdk::{
    account::Account, commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Signature,
};
use solana_transaction_status::{
    EncodedConfirmedTransactionWithStatusMeta, EncodedTransaction, UiLoadedAddresses, UiMessage,
    UiTransactionEncoding,
};
use tokio::sync::mpsc;

use crate::core::commitment::TransactionCommitment;
use crate::core::error::SvmIndexerError;
use crate::indexer::rpc::{
    AccountFilter, AccountSnapshot, LogsNotification, RawTransaction, RpcResult, SignatureInfo,
    SubReady, SubStream, SvmRpc, TxAccount,
};

const MAX_MULTIPLE_ACCOUNTS: usize = 100;

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

    async fn connect(&self) -> RpcResult<PubsubClient> {
        PubsubClient::new(&self.ws_url)
            .await
            .map_err(|err| SvmIndexerError::SubscriptionFailed(err.to_string()))
    }
}

const SUB_CHANNEL_CAPACITY: usize = 1024;

fn rpc_error(err: ClientError) -> SvmIndexerError {
    match err.kind() {
        ClientErrorKind::RpcError(RpcError::RpcResponseError { code, .. }) => {
            SvmIndexerError::RpcResponseError(*code, err.to_string())
        }
        _ => SvmIndexerError::RpcRequestFailed(err.to_string()),
    }
}

/// Wraps a channel receiver as an owned `'static` stream — nothing borrowed, so the caller can hold
/// it without pinning the pubsub client alive.
fn receiver_stream<T: Send + 'static>(rx: mpsc::Receiver<RpcResult<T>>) -> SubStream<T> {
    Box::pin(futures::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|item| (item, rx))
    }))
}

#[async_trait]
impl SvmRpc for JsonRpc {
    async fn get_slot(&self) -> RpcResult<u64> {
        self.client.get_slot().await.map_err(rpc_error)
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
        let config = RpcProgramAccountsConfig {
            filters: rpc_filters(filter),
            account_config: RpcAccountInfoConfig {
                encoding: Some(UiAccountEncoding::Base64Zstd),
                commitment: Some(self.commitment),
                ..Default::default()
            },
            with_context: Some(true),
            ..Default::default()
        };
        let response: OptionalContext<Vec<RpcKeyedAccount>> = self
            .client
            .send(
                RpcRequest::GetProgramAccounts,
                serde_json::json!([program.to_string(), config]),
            )
            .await
            .map_err(rpc_error)?;
        let (slot, accounts) = match response {
            OptionalContext::Context(response) => (response.context.slot, response.value),
            // a node that ignores `withContext` still answers, just without a slot to attribute it to
            OptionalContext::NoContext(accounts) => (self.get_slot().await?, accounts),
        };

        accounts
            .into_iter()
            .map(|keyed| {
                let address = keyed.pubkey.parse().map_err(|_| {
                    SvmIndexerError::RpcRequestFailed(format!(
                        "invalid account key {}",
                        keyed.pubkey
                    ))
                })?;
                let account = keyed.account.decode::<Account>().ok_or_else(|| {
                    SvmIndexerError::RpcRequestFailed(format!(
                        "undecodable account {}",
                        keyed.pubkey
                    ))
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
            })
            .collect()
    }

    async fn get_multiple_accounts(&self, addresses: &[Pubkey]) -> RpcResult<Vec<AccountSnapshot>> {
        let config = RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64Zstd),
            ..Default::default()
        };
        let mut snapshots = Vec::new();

        for chunk in addresses.chunks(MAX_MULTIPLE_ACCOUNTS) {
            let response = self
                .client
                .get_multiple_accounts_with_config(chunk, config.clone())
                .await
                .map_err(rpc_error)?;
            let slot = response.context.slot;
            snapshots.extend(
                chunk
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
                    }),
            );
        }
        Ok(snapshots)
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
            commitment: Some(self.commitment),
        };
        let signatures = self
            .client
            .get_signatures_for_address_with_config(address, config)
            .await
            .map_err(rpc_error)?;

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
            commitment: Some(self.commitment),
            max_supported_transaction_version: Some(0),
        };
        let tx = self
            .client
            .get_transaction_with_config(signature, config)
            .await
            .map_err(rpc_error)?;

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

    async fn get_signature_statuses(
        &self,
        signatures: &[Signature],
    ) -> RpcResult<Vec<Option<TransactionCommitment>>> {
        let statuses = self
            .client
            .get_signature_statuses(signatures)
            .await
            .map_err(rpc_error)?
            .value;
        Ok(statuses
            .into_iter()
            .map(|status| {
                status.map(|status| {
                    status
                        .confirmation_status
                        .as_ref()
                        .map(TransactionCommitment::from)
                        .unwrap_or(TransactionCommitment::Processed)
                })
            })
            .collect())
    }

    async fn get_signature_statuses_with_history(
        &self,
        signatures: &[Signature],
    ) -> RpcResult<Vec<Option<TransactionCommitment>>> {
        let statuses = self
            .client
            .get_signature_statuses_with_history(signatures)
            .await
            .map_err(rpc_error)?
            .value;
        Ok(statuses
            .into_iter()
            .map(|status| {
                status.map(|status| {
                    status
                        .confirmation_status
                        .as_ref()
                        .map(TransactionCommitment::from)
                        .unwrap_or(TransactionCommitment::Processed)
                })
            })
            .collect())
    }

    async fn subscribe_program(
        &self,
        program: &Pubkey,
        filter: Option<&AccountFilter>,
        ready: SubReady,
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
        let program = *program;
        let (tx, rx) = mpsc::channel(SUB_CHANNEL_CAPACITY);
        tokio::spawn(async move {
            let (mut stream, _unsub) = match client.program_subscribe(&program, Some(config)).await
            {
                Ok(pair) => pair,
                Err(err) => {
                    let _ = tx
                        .send(Err(SvmIndexerError::SubscriptionFailed(err.to_string())))
                        .await;
                    return;
                }
            };
            let _ = ready.send(());
            loop {
                let response = tokio::select! {
                    _ = tx.closed() => break, // consumer gone; drop the subscription and close the socket
                    response = stream.next() => match response {
                        Some(response) => response,
                        None => break,
                    },
                };
                let slot = response.context.slot;
                let keyed = response.value;
                let item = keyed
                    .pubkey
                    .parse()
                    .map_err(|_| SvmIndexerError::SubscriptionFailed(keyed.pubkey.clone()))
                    .and_then(|address| {
                        let account = keyed.account.decode::<Account>().ok_or_else(|| {
                            SvmIndexerError::SubscriptionFailed(format!(
                                "undecodable account {}",
                                keyed.pubkey
                            ))
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
                    });
                if tx.send(item).await.is_err() {
                    break;
                }
            }
        });
        Ok(receiver_stream(rx))
    }

    async fn subscribe_accounts(
        &self,
        addresses: &[Pubkey],
        ready: SubReady,
    ) -> RpcResult<SubStream<AccountSnapshot>> {
        let client = self.connect().await?;
        let config = RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64Zstd),
            commitment: Some(self.commitment),
            ..Default::default()
        };
        let addresses = addresses.to_vec();
        let (tx, rx) = mpsc::channel(SUB_CHANNEL_CAPACITY);
        tokio::spawn(async move {
            let mut streams = Vec::with_capacity(addresses.len());
            for address in addresses {
                // accountSubscribe notifications carry no pubkey, so tag each stream with its known key
                let (stream, _unsub) = match client
                    .account_subscribe(&address, Some(config.clone()))
                    .await
                {
                    Ok(pair) => pair,
                    Err(err) => {
                        let _ = tx
                            .send(Err(SvmIndexerError::SubscriptionFailed(err.to_string())))
                            .await;
                        return;
                    }
                };
                streams.push(stream.map(move |response| {
                    let account = response.value.decode::<Account>().ok_or_else(|| {
                        SvmIndexerError::SubscriptionFailed(format!(
                            "undecodable account {address}"
                        ))
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
                }));
            }
            let mut merged = futures::stream::select_all(streams);
            let _ = ready.send(()); // only once every key is subscribed
            loop {
                let item = tokio::select! {
                    _ = tx.closed() => break,
                    item = merged.next() => match item {
                        Some(item) => item,
                        None => break,
                    },
                };
                if tx.send(item).await.is_err() {
                    break;
                }
            }
        });
        Ok(receiver_stream(rx))
    }

    async fn subscribe_logs(
        &self,
        program: &Pubkey,
        ready: SubReady,
    ) -> RpcResult<SubStream<LogsNotification>> {
        let client = self.connect().await?;
        let config = RpcTransactionLogsConfig {
            commitment: Some(self.commitment),
        };
        let filter = RpcTransactionLogsFilter::Mentions(vec![program.to_string()]);
        let (tx, rx) = mpsc::channel(SUB_CHANNEL_CAPACITY);
        tokio::spawn(async move {
            let (mut stream, _unsub) = match client.logs_subscribe(filter, config).await {
                Ok(pair) => pair,
                Err(err) => {
                    let _ = tx
                        .send(Err(SvmIndexerError::SubscriptionFailed(err.to_string())))
                        .await;
                    return;
                }
            };
            let _ = ready.send(());
            loop {
                let response = tokio::select! {
                    _ = tx.closed() => break,
                    response = stream.next() => match response {
                        Some(response) => response,
                        None => break,
                    },
                };
                let slot = response.context.slot;
                let value = response.value;
                let item = value
                    .signature
                    .parse()
                    .map_err(|_| SvmIndexerError::SubscriptionFailed(value.signature.clone()))
                    .map(|signature| LogsNotification {
                        signature,
                        slot,
                        err: value
                            .err
                            .map(|e| serde_json::to_value(e).unwrap_or_default()),
                        logs: value.logs,
                    });
                if tx.send(item).await.is_err() {
                    break;
                }
            }
        });
        Ok(receiver_stream(rx))
    }
}

/// Server-side filters for a descriptor, or `None` when nothing narrows — in which case the RPC
/// returns/streams every account the program owns.
fn rpc_filters(filter: Option<&AccountFilter>) -> Option<Vec<RpcFilterType>> {
    let filters = filter?.rpc_filters();
    (!filters.is_empty()).then_some(filters)
}

/// fee payer is always the first account key
pub(crate) fn fee_payer(tx: &EncodedConfirmedTransactionWithStatusMeta) -> Option<Pubkey> {
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
pub(crate) fn account_keys(tx: &EncodedConfirmedTransactionWithStatusMeta) -> Vec<TxAccount> {
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
        let writable = loaded.writable.iter().filter_map(|key| {
            Some(TxAccount {
                address: key.parse().ok()?,
                writable: true,
            })
        });
        let readonly = loaded.readonly.iter().filter_map(|key| {
            Some(TxAccount {
                address: key.parse().ok()?,
                writable: false,
            })
        });
        accounts.extend(writable.chain(readonly));
    }
    accounts
}
