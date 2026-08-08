use async_trait::async_trait;
use solana_account_decoder::UiAccountEncoding;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config;
use solana_rpc_client_api::config::{
    RpcAccountInfoConfig, RpcProgramAccountsConfig, RpcTransactionConfig,
};
use solana_sdk::{pubkey::Pubkey, signature::Signature};
use solana_transaction_status::{
    EncodedConfirmedTransactionWithStatusMeta, EncodedTransaction, UiMessage, UiTransactionEncoding,
};

use crate::core::error::SvmIndexerError;
use crate::indexer::rpc::{
    AccountFilter, AccountSnapshot, LogsNotification, RawTransaction, RpcResult, SignatureInfo,
    SubStream, SvmRpc,
};

pub struct JsonRpc {
    client: RpcClient,
}

impl JsonRpc {
    pub fn new(http_url: String) -> Self {
        Self {
            client: RpcClient::new(http_url),
        }
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
        _filter: Option<&AccountFilter>,
    ) -> RpcResult<Vec<AccountSnapshot>> {
        let slot = self
            .client
            .get_slot()
            .await
            .map_err(|err| SvmIndexerError::RpcRequestFailed(err.to_string()))?;
        let config = RpcProgramAccountsConfig {
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
            raw: serde_json::to_value(&tx)
                .map_err(|err| SvmIndexerError::SerializationError(err.to_string()))?,
        })
    }

    async fn subscribe_program(
        &self,
        _program: &Pubkey,
        _filter: Option<&AccountFilter>,
    ) -> RpcResult<SubStream<AccountSnapshot>> {
        todo!()
    }

    async fn subscribe_logs(&self, _program: &Pubkey) -> RpcResult<SubStream<LogsNotification>> {
        todo!()
    }
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
