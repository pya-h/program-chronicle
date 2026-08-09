use base64::{Engine, engine::general_purpose::STANDARD};
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
    pub slot: i64,
    pub success: bool,
    pub err: Option<Value>,
    pub fee: Option<i64>,
    pub fee_payer: Option<String>,
    pub block_time: Option<i64>,
    pub raw: Value,
}

impl TransactionResponse {
    pub fn from_model(tx: TransactionModel, block_time: Option<i64>) -> Self {
        Self {
            signature: signature_string(&tx.signature),
            program_id: tx.program_id,
            slot: tx.slot,
            success: tx.success,
            err: tx.error,
            fee: tx.fee,
            fee_payer: tx.fee_payer.map(|payer| pubkey_string(&payer)),
            block_time,
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
    pub fee: Option<i64>,
    pub fee_payer: Option<String>,
    pub block_time: Option<i64>,
}

impl TransactionSummaryResponse {
    pub fn from_model(tx: TransactionModel, block_time: Option<i64>) -> Self {
        Self {
            signature: signature_string(&tx.signature),
            program_id: tx.program_id,
            slot: tx.slot,
            success: tx.success,
            err: tx.error,
            fee: tx.fee,
            fee_payer: tx.fee_payer.map(|payer| pubkey_string(&payer)),
            block_time,
        }
    }
}

#[derive(Serialize)]
pub struct TransactionsResponse {
    pub data: Vec<TransactionSummaryResponse>,
    pub total: u16,
    pub pages: u16,
}

impl TransactionsResponse {
    pub fn new(rows: Vec<(TransactionModel, Option<SlotModel>)>, total: u16, pages: u16) -> Self {
        Self {
            total,
            pages,
            data: rows
                .into_iter()
                .map(|(tx, slot)| {
                    TransactionSummaryResponse::from_model(tx, slot.and_then(|slot| slot.block_time))
                })
                .collect(),
        }
    }
}
