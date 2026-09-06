use axum::{Json, http::StatusCode, response};
use thiserror::Error;

/// `getProgramAccounts` refused: too many accounts accumulated for the node to serve in one scan.
const SCAN_LIMIT_EXCEEDED: i64 = -32012;

/// The four ways `getBlock` answers "there is no block here", reported as errors rather than as an
/// empty result. The code says *why*, which is what separates a reorg from a gap in the node.
const BLOCK_PRUNED: i64 = -32001;
const BLOCK_NOT_YET_AVAILABLE: i64 = -32004;
const SLOT_SKIPPED: i64 = -32007;
const ARCHIVED_SLOT_SKIPPED: i64 = -32009;

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

    #[error("{0}")]
    Conflict(String),

    #[error("Client Error! {0}")]
    ProgramFailure(String),

    #[error("Client Error! {0}")]
    SolanaClientFailure(String),

    #[error("Unknown Client error! {0}")]
    UnknownSolanaClientError(String),

    #[error("RPC request failed! {0}")]
    RpcRequestFailed(String),

    /// with the code
    #[error("RPC request failed! {1}")]
    RpcResponseError(i64, String),

    #[error("RPC subscription failed! {0}")]
    SubscriptionFailed(String),

    #[error("Indexer failure! {0}")]
    IndexerFailure(String),

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
                    format!("{} [id = {}]", self, id)
                } else {
                    tracing::error!(error = %seaorm_err, message = %public_message);
                    self.to_string()
                };
                (StatusCode::INTERNAL_SERVER_ERROR, response_msg)
            }
            SvmIndexerError::BadInput(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            SvmIndexerError::Conflict(_) => (StatusCode::CONFLICT, self.to_string()),
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

    /// The node refused to scan the program because too many accounts matched — the failure a
    /// partitioned scan works around, and the only one worth retrying differently.
    pub fn is_scan_limit(&self) -> bool {
        matches!(
            self,
            SvmIndexerError::RpcResponseError(SCAN_LIMIT_EXCEEDED, _)
        )
    }

    pub fn is_block_pruned(&self) -> bool {
        matches!(self, SvmIndexerError::RpcResponseError(BLOCK_PRUNED, _))
    }

    pub fn is_block_not_yet_available(&self) -> bool {
        matches!(
            self,
            SvmIndexerError::RpcResponseError(BLOCK_NOT_YET_AVAILABLE, _)
        )
    }

    /// No block at this slot on the node's canonical chain. The two codes are the same fact from
    /// two storage layers — the local blockstore and the archival backend behind it.
    pub fn is_slot_skipped(&self) -> bool {
        matches!(
            self,
            SvmIndexerError::RpcResponseError(SLOT_SKIPPED | ARCHIVED_SLOT_SKIPPED, _)
        )
    }
}
