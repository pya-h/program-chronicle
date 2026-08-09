use serde::Serialize;

use super::{accounts::AccountChangeResponse, events::ProgramEventResponse};

#[derive(Serialize)]
pub struct ProgramChangesResponse {
    pub account_states: Vec<AccountChangeResponse>,
    pub account_states_total: u64,
    pub account_states_pages: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<ProgramEventResponse>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events_total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events_pages: Option<u64>,
}
