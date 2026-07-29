use axum::{Json, http::StatusCode, response};
use serde_json::json;
use thiserror::Error;

#[allow(unused)] // TODO: Remove this in the end
#[derive(Error, Debug)]
pub enum SvmIndexerError {
    #[error("Provided PublicKey has invalid format! {0}")]
    InvalidPublicKey(String),

    #[error("Something went wrong in Serialization step! {0}")]
    SerializationError(String),

    #[error("Account not found! {0}")]
    AccountNotFound(String),

    #[error("Client Error! {0}")]
    ProgramFailure(String),

    #[error("Client Error! {0}")]
    SolanaClientFailure(String),

    #[error("Unknown Client error! {0}")]
    UnknownSolanaClientError(String),

    #[error("Invalid URL input! {0}")]
    InvalidURL(String),

    #[error("Internal Error happened: {0}")]
    InternalError(String),
}

pub type FinalResponse<T> = Result<Json<T>, SvmIndexerError>;

impl response::IntoResponse for SvmIndexerError {
    fn into_response(self) -> response::Response {
        let (status, message) = match &self {
            SvmIndexerError::InvalidPublicKey(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            SvmIndexerError::AccountNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            SvmIndexerError::ProgramFailure(_) => (StatusCode::BAD_GATEWAY, self.to_string()),
            SvmIndexerError::InvalidURL(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            SvmIndexerError::InternalError(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
        };
        (status, Json(json!({"message": message}))).into_response()
    }
}
