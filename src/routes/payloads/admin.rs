use serde::Deserialize;


#[derive(Debug, Deserialize)]
pub struct AddRpcPayload {
    pub http_url: String,
    pub ws_url: Option<String>
}