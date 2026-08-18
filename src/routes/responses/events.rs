use base64::{Engine, engine::general_purpose::STANDARD};
use sea_orm::entity::prelude::DateTime;
use serde::Serialize;
use solana_sdk::signature::Signature;

use crate::db::entities::{program_events::Model as ProgramEventModel, slots::Model as SlotModel};

fn signature_string(bytes: &[u8]) -> String {
    Signature::try_from(bytes)
        .map(|sig| sig.to_string())
        .unwrap_or_else(|_| STANDARD.encode(bytes))
}

#[derive(Serialize)]
pub struct ProgramEventResponse {
    pub tx_signature: String,
    pub log_index: i32,
    pub data: String,
    pub slot: i64,
    pub block_time: Option<i64>,
    pub created_at: DateTime,
}

impl ProgramEventResponse {
    pub fn from_model(event: ProgramEventModel, block_time: Option<i64>) -> Self {
        Self {
            tx_signature: signature_string(&event.tx_signature),
            log_index: event.log_index,
            data: STANDARD.encode(&event.data),
            slot: event.slot,
            block_time,
            created_at: event.created_at,
        }
    }
}

#[derive(Serialize)]
pub struct ProgramEventsResponse {
    pub data: Vec<ProgramEventResponse>,
    pub total: u64,
    pub pages: u64,
}

impl ProgramEventsResponse {
    pub fn new(rows: Vec<(ProgramEventModel, Option<SlotModel>)>, total: u64, pages: u64) -> Self {
        Self {
            total,
            pages,
            data: rows
                .into_iter()
                .map(|(event, slot)| {
                    ProgramEventResponse::from_model(event, slot.and_then(|slot| slot.block_time))
                })
                .collect(),
        }
    }
}
