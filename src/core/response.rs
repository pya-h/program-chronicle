use super::error::SvmIndexerError;
use axum::{
    Json,
    http::StatusCode,
    response::{self, NoContent},
};
use serde::Serialize;

pub struct ApiResponse<T: Serialize>(pub StatusCode, pub Json<T>);

pub type FinalResponse<T> = Result<ApiResponse<T>, SvmIndexerError>;

// shortcuts:
pub type NoContentResponse = Result<NoContent, SvmIndexerError>;

impl<T: Serialize> ApiResponse<T> {
    pub fn new(status: StatusCode, body: T) -> Self {
        Self(status, Json(body))
    }
    // some useful shortcuts
    pub fn ok(body: T) -> Self {
        Self(StatusCode::OK, Json(body))
    }

    pub fn created(body: T) -> Self {
        Self(StatusCode::CREATED, Json(body))
    }
}

impl<T: Serialize> response::IntoResponse for ApiResponse<T> {
    fn into_response(self) -> response::Response {
        (self.0, self.1).into_response()
    }
}

impl<T: Serialize> From<T> for ApiResponse<T> {
    fn from(body: T) -> Self {
        Self::ok(body)
    }
}
