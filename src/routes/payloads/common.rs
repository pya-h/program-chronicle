use serde::Deserialize;

#[derive(Deserialize)]
pub struct PaginationPayload {
    pub skip: u16,
    pub offset: u16,
}
