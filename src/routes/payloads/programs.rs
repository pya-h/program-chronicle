use serde::Deserialize;

use crate::routes::payloads::common::PaginationQuery;

#[derive(Deserialize)]
pub struct ProgramAccountsQuery {
    pub memcmp: Option<String>,
    #[serde(rename = "dataSize")]
    pub data_size: Option<i64>,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}

#[derive(Deserialize)]
pub struct ProgramEventsQuery {
    pub since_slot: Option<i64>,
    pub from_slot: Option<i64>,
    pub to_slot: Option<i64>,
    pub tx_signature: Option<String>,
    pub discriminator: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}

#[derive(Deserialize)]
pub struct ProgramTransactionsQuery {
    pub since_slot: Option<i64>,
    pub from_slot: Option<i64>,
    pub to_slot: Option<i64>,
    pub success: Option<bool>,
    pub commitment: Option<String>,
    pub correlated: Option<bool>,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}
