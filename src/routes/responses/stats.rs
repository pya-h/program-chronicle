use base64::{Engine, engine::general_purpose::STANDARD};
use sea_orm::entity::prelude::DateTime;
use serde::Serialize;
use solana_sdk::pubkey::Pubkey;

use crate::core::state::{ProgramPhase, ProgramRuntime};
use crate::db::entities::programs::Model as ProgramModel;

fn pubkey_string(bytes: &[u8]) -> String {
    Pubkey::try_from(bytes)
        .map(|pk| pk.to_string())
        .unwrap_or_else(|_| STANDARD.encode(bytes))
}

#[derive(Serialize)]
pub struct ProgramProgressResponse {
    pub id: i32,
    pub name: Option<String>,
    pub address: String,
    pub start_slot: Option<i64>,
    pub last_processed_slot: Option<i64>,
    pub last_processed_at: Option<DateTime>,
    pub phase: Option<ProgramPhase>,
    /// Chain tip of this program's own rpc, so progress is measured per program instead of against
    /// whichever program happens to sit on the highest-slot chain.
    pub head_slot: Option<i64>,
}

impl ProgramProgressResponse {
    pub fn from_model(program: ProgramModel, runtime: Option<ProgramRuntime>) -> Self {
        Self {
            id: program.id,
            name: program.name,
            address: pubkey_string(&program.address),
            start_slot: program.start_slot,
            last_processed_slot: program.last_processed_slot,
            last_processed_at: program.last_processed_at,
            phase: runtime.map(|runtime| runtime.phase),
            head_slot: runtime.and_then(|runtime| runtime.head_slot),
        }
    }
}

/// Row counts sourced from `pg_stat_user_tables.n_live_tup` (live estimate) — never an exact
/// `COUNT(*)`.
#[derive(Serialize)]
pub struct TableEstimates {
    pub slots: i64,
    pub transactions: i64,
    pub events: i64,
    pub accounts: i64,
}

#[derive(Serialize)]
pub struct SettlementCounts {
    pub reorged_slots: u64,
    pub dropped_transactions: u64,
}

#[derive(Serialize)]
pub struct IndexerStatsResponse {
    pub programs: u64,
    pub rpcs: u64,
    pub estimates: TableEstimates,
    pub settlement: SettlementCounts,
    pub per_program: Vec<ProgramProgressResponse>,
}
