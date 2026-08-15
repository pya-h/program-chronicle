use serde::Deserialize;

use crate::routes::payloads::common::PaginationQuery;

#[derive(Deserialize)]
pub struct MultiAccountQuery {
    pub addresses: String,
}

#[derive(Deserialize)]
pub struct AccountTransactionsQuery {
    pub since_slot: Option<i64>,
    pub from_slot: Option<i64>,
    pub to_slot: Option<i64>,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}
