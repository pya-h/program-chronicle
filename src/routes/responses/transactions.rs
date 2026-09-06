use base64::{Engine, engine::general_purpose::STANDARD};
use sea_orm::entity::prelude::DateTime;
use serde::Serialize;
use serde_json::Value;
use solana_sdk::{pubkey::Pubkey, signature::Signature};

use crate::db::entities::{slots::Model as SlotModel, transactions::Model as TransactionModel};

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

#[derive(Serialize)]
pub struct TransactionResponse {
    pub signature: String,
    pub program_id: Option<i32>,
    /// Every registered program participating in the tx, derived from the correlation tables;
    /// `program_id` above is only the first ingester (lossy when programs share a transaction).
    pub programs: Vec<i32>,
    pub slot: i64,
    pub success: bool,
    pub err: Option<Value>,
    pub commitment: String,
    pub fee: Option<i64>,
    pub fee_payer: Option<String>,
    pub block_time: Option<i64>,
    pub created_at: DateTime,
    pub raw: Value,
}

impl TransactionResponse {
    pub fn from_model(tx: TransactionModel, block_time: Option<i64>, programs: Vec<i32>) -> Self {
        Self {
            signature: signature_string(&tx.signature),
            program_id: tx.program_id,
            programs,
            slot: tx.slot,
            success: tx.success,
            err: tx.error,
            commitment: tx.commitment,
            fee: tx.fee,
            fee_payer: tx.fee_payer.map(|payer| pubkey_string(&payer)),
            block_time,
            created_at: tx.created_at,
            raw: tx.raw,
        }
    }
}

#[derive(Serialize)]
pub struct TransactionSummaryResponse {
    pub signature: String,
    pub program_id: Option<i32>,
    pub slot: i64,
    pub success: bool,
    pub err: Option<Value>,
    pub commitment: String,
    pub fee: Option<i64>,
    pub fee_payer: Option<String>,
    pub block_time: Option<i64>,
    pub created_at: DateTime,
}

impl TransactionSummaryResponse {
    pub fn from_model(tx: TransactionModel, block_time: Option<i64>) -> Self {
        Self {
            signature: signature_string(&tx.signature),
            program_id: tx.program_id,
            slot: tx.slot,
            success: tx.success,
            err: tx.error,
            commitment: tx.commitment,
            fee: tx.fee,
            fee_payer: tx.fee_payer.map(|payer| pubkey_string(&payer)),
            block_time,
            created_at: tx.created_at,
        }
    }
}

#[derive(Serialize)]
pub struct TransactionsResponse {
    pub data: Vec<TransactionSummaryResponse>,
    pub total: u64,
    pub pages: u64,
}

impl TransactionsResponse {
    pub fn new(rows: Vec<(TransactionModel, Option<SlotModel>)>, total: u64, pages: u64) -> Self {
        Self {
            total,
            pages,
            data: rows
                .into_iter()
                .map(|(tx, slot)| {
                    TransactionSummaryResponse::from_model(
                        tx,
                        slot.and_then(|slot| slot.block_time),
                    )
                })
                .collect(),
        }
    }
}
