use serde::Deserialize;

use crate::indexer::rpc::AccountFilter;
use crate::routes::payloads::common::{PaginationQuery, de_double_option, de_opt_bool};

#[derive(Deserialize)]
pub struct IndexerErrorsQuery {
    #[serde(flatten)]
    pub pagination: PaginationQuery,
    #[serde(default, deserialize_with = "de_opt_bool")]
    pub seen: Option<bool>,
}

#[derive(Deserialize)]
pub struct AddRpcPayload {
    pub http_url: String,
    pub ws_url: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateRpcPayload {
    pub http_url: Option<String>,
    pub ws_url: Option<String>,
}

#[derive(Deserialize)]
pub struct AddProgramPayload {
    pub rpc_id: Option<i32>,
    pub rpc_url: Option<AddRpcPayload>,
    pub address: String,
    pub name: Option<String>,
    pub start_slot: Option<u64>,
    /// Optional ingestion narrowing; absent/empty tracks every account the program owns.
    pub account_filters: Option<AccountFilter>,
    pub scan_partition_offset: Option<i32>,
}

#[derive(Deserialize)]
pub struct GetSingleProgramPayload {
    pub ignore_unknown_rpc: Option<bool>, // In case rpc data failed loading, due to any reason
}

#[derive(Deserialize)]
pub struct UpdateProgramPayload {
    pub rpc_id: Option<i32>,
    pub rpc_url: Option<AddRpcPayload>,
    pub address: Option<String>,
    pub name: Option<String>,
    /// Absent keeps the current filter; an empty descriptor clears it (back to track-all).
    pub account_filters: Option<AccountFilter>,
    /// Double option: absent keeps the current offset, an explicit `null` clears it.
    #[serde(default, deserialize_with = "de_double_option")]
    pub scan_partition_offset: Option<Option<i32>>,
}

impl UpdateProgramPayload {
    pub fn is_empty(&self) -> bool {
        self.rpc_id.is_none()
            && self.rpc_url.is_none()
            && self.address.is_none()
            && self.name.is_none()
            && self.account_filters.is_none()
            && self.scan_partition_offset.is_none()
    }
}
