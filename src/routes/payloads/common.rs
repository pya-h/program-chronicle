use serde::Deserialize;

#[derive(Deserialize)]
pub struct PaginationPayload {
    pub per_page: Option<u16>,
    pub page: Option<u16>,
}
