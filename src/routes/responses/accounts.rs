use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Serialize;
use solana_sdk::pubkey::Pubkey;

use crate::db::entities::program_accounts::Model as ProgramAccountModel;

fn pubkey_string(bytes: &[u8]) -> String {
    Pubkey::try_from(bytes)
        .map(|pk| pk.to_string())
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
