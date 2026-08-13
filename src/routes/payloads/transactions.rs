use serde::Deserialize;

use crate::routes::payloads::common::PaginationQuery;

#[derive(Deserialize)]
pub struct TransactionsQuery {
    pub since_slot: Option<i64>,
    pub from_slot: Option<i64>,
    pub to_slot: Option<i64>,
    pub commitment: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}
