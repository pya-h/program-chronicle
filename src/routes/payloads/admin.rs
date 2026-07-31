use serde::Deserialize;


#[derive(Deserialize)]
pub struct AddRpcPayload {
    pub http_url: String,
    pub ws_url: Option<String>
}