use axum::{Json, http::StatusCode, response};
use thiserror::Error;

#[allow(unused)] // TODO: Remove this in the end
#[derive(Error, Debug)]
pub enum SvmIndexerError {
    #[error("Invalid PublicKey format! {0}")]
    InvalidPublicKey(String),

    #[error("Something went wrong in Serialization step! {0}")]
    SerializationError(String),

    #[error("Item not found! {0}")]
    ItemNotFound(String),

    #[error("Invalid Parameters! {0}")]
    BadInput(String),

    #[error("Client Error! {0}")]
    ProgramFailure(String),

    #[error("Client Error! {0}")]
    SolanaClientFailure(String),

    #[error("Unknown Client error! {0}")]
    UnknownSolanaClientError(String),

    #[error("Invalid URL input! {0}")]
    InvalidURL(String),

    #[error("Access denied!")]
    Unauthorized,

    #[error("Database level error happened!: {0}")]
    DatabaseError(String, sea_orm::DbErr, Option<u64>),

    #[error("Internal Error happened: {0}")]
    InternalError(String),
}

impl response::IntoResponse for SvmIndexerError {
    fn into_response(self) -> response::Response {
        let (status, message) = match &self {
            SvmIndexerError::InvalidPublicKey(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            SvmIndexerError::ItemNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            SvmIndexerError::ProgramFailure(_) => (StatusCode::BAD_GATEWAY, self.to_string()),
            SvmIndexerError::InvalidURL(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            SvmIndexerError::InternalError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, self.to_string())
            }
            SvmIndexerError::Unauthorized => (StatusCode::UNAUTHORIZED, self.to_string()),
            SvmIndexerError::DatabaseError(public_message, seaorm_err, element_id) => {
                let response_msg = if let Some(id) = element_id {
                    tracing::error!(error = %seaorm_err, message = %public_message, id = id);
                    format!("{} [id = {}]", self.to_string(), id)
                } else {
                    tracing::error!(error = %seaorm_err, message = %public_message);
                    self.to_string()
                };
                (StatusCode::INTERNAL_SERVER_ERROR, response_msg)
            }
            SvmIndexerError::BadInput(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
        };
        (status, Json(serde_json::json!({"message": message}))).into_response()
    }
}

impl From<sea_orm::DbErr> for SvmIndexerError {
    fn from(err: sea_orm::DbErr) -> Self {
        SvmIndexerError::DatabaseError(String::from("Unknown error occured!"), err, None)
    }
}

impl SvmIndexerError {
    pub fn db_error(message: &str, sea_orm_err: sea_orm::DbErr) -> Self {
        SvmIndexerError::DatabaseError(String::from(message), sea_orm_err, None)
    }

    pub fn db_error_with_id(message: &str, sea_orm_err: sea_orm::DbErr, id: u64) -> Self {
        SvmIndexerError::DatabaseError(String::from(message), sea_orm_err, Some(id))
    }
}