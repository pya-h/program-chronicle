use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Serialize;
use solana_sdk::{pubkey::Pubkey, signature::Signature};

use crate::db::entities::{
    account_states::Model as AccountStateModel, program_accounts::Model as ProgramAccountModel,
    slots::Model as SlotModel,
};

fn pubkey_string(bytes: &[u8]) -> String {
    Pubkey::try_from(bytes)
        .map(|pk| pk.to_string())
        .unwrap_or_else(|_| STANDARD.encode(bytes))
}

fn signature_string(bytes: &[u8]) -> String {
    Signature::try_from(bytes)
        .map(|sig| sig.to_string())
        .unwrap_or_else(|_| STANDARD.encode(bytes))
}

#[derive(Serialize, Clone)]
pub struct AccountInfoResponse {
    pub address: String,
    pub owner: String,
    pub data: String,
    pub lamports: i64,
    pub rent_epoch: Option<i64>,
    pub executable: bool, // always false: just for keeping similarity to blockchain response
    pub slot: i64,
    pub write_version: i64,
    pub block_time: Option<i64>,
}

impl AccountInfoResponse {
    pub fn from_model(account: ProgramAccountModel, block_time: Option<i64>) -> Self {
        Self {
            address: pubkey_string(&account.address),
            owner: pubkey_string(&account.owner),
            data: STANDARD.encode(&account.state),
            lamports: account.lamports,
            rent_epoch: account.rent_epoch,
            executable: false,
            slot: account.latest_slot,
            write_version: account.latest_write_version,
            block_time,
        }
    }
}

#[derive(Serialize)]
pub struct ProgramAccountsResponse {
    pub data: Vec<AccountInfoResponse>,
    pub total: u64,
    pub pages: u64,
}

impl ProgramAccountsResponse {
    pub fn new(rows: Vec<(ProgramAccountModel, Option<SlotModel>)>, total: u64, pages: u64) -> Self {
        Self {
            total,
            pages,
            data: rows
                .into_iter()
                .map(|(account, slot)| {
                    AccountInfoResponse::from_model(account, slot.and_then(|slot| slot.block_time))
                })
                .collect(),
        }
    }
}

#[derive(Serialize)]
pub struct AccountStateResponse {
    pub owner: String,
    pub data: String,
    pub lamports: i64,
    pub slot: i64,
    pub write_version: i64,
    pub tx_signature: Option<String>,
    pub block_time: Option<i64>,
}

impl AccountStateResponse {
    pub fn from_model(state: AccountStateModel, block_time: Option<i64>) -> Self {
        Self {
            owner: pubkey_string(&state.owner),
            data: STANDARD.encode(&state.data),
            lamports: state.lamports,
            slot: state.slot,
            write_version: state.write_version,
            tx_signature: state.tx_signature.map(|sig| signature_string(&sig)),
            block_time,
        }
    }
}

#[derive(Serialize)]
pub struct AccountChangeResponse {
    pub address: String,
    pub owner: String,
    pub data: String,
    pub lamports: i64,
    pub slot: i64,
    pub write_version: i64,
    pub tx_signature: Option<String>,
    pub block_time: Option<i64>,
}

impl AccountChangeResponse {
    pub fn from_model(state: AccountStateModel, address: &[u8], block_time: Option<i64>) -> Self {
        Self {
            address: pubkey_string(address),
            owner: pubkey_string(&state.owner),
            data: STANDARD.encode(&state.data),
            lamports: state.lamports,
            slot: state.slot,
            write_version: state.write_version,
            tx_signature: state.tx_signature.map(|sig| signature_string(&sig)),
            block_time,
        }
    }
}

#[derive(Serialize)]
pub struct AccountHistoryResponse {
    pub data: Vec<AccountStateResponse>,
    pub total: u64,
    pub pages: u64,
}

impl AccountHistoryResponse {
    pub fn new(rows: Vec<(AccountStateModel, Option<SlotModel>)>, total: u64, pages: u64) -> Self {
        Self {
            total,
            pages,
            data: rows
                .into_iter()
                .map(|(state, slot)| {
                    AccountStateResponse::from_model(state, slot.and_then(|slot| slot.block_time))
                })
                .collect(),
        }
    }
}
