use serde::Deserialize;

#[derive(Deserialize)]
pub struct AddRpcPayload {
    pub http_url: String,
    pub ws_url: Option<String>
}


#[derive(Deserialize)]
pub struct UpdateRpcPayload {
    pub http_url: Option<String>,
    pub ws_url: Option<String>
}

#[derive(Deserialize)]
pub struct AddProgramPayload {
    pub rpc_id: Option<u16>,
    pub rpc_url: Option<AddRpcPayload>,
    pub address: String,
    pub name: Option<String>,
    pub start_slot: Option<u64>,
}

#[derive(Deserialize)]
pub struct GetSingleProgramPayload {
    pub ignore_unknown_rpc: Option<bool> // In case rpc data failed loading, due to any reason
}