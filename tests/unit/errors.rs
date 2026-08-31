use axum::body::to_bytes;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use sea_orm::DbErr;

use crate::core::error::SvmIndexerError;

fn status_of(err: SvmIndexerError) -> StatusCode {
    err.into_response().status()
}

#[test]
fn maps_variants_to_expected_status_codes() {
    assert_eq!(
        status_of(SvmIndexerError::InvalidPublicKey("x".into())),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        status_of(SvmIndexerError::BadInput("x".into())),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        status_of(SvmIndexerError::InvalidURL("x".into())),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        status_of(SvmIndexerError::ItemNotFound("x".into())),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        status_of(SvmIndexerError::Conflict("x".into())),
        StatusCode::CONFLICT
    );
    assert_eq!(
        status_of(SvmIndexerError::Unauthorized),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        status_of(SvmIndexerError::ProgramFailure("x".into())),
        StatusCode::BAD_GATEWAY
    );
    assert_eq!(
        status_of(SvmIndexerError::InternalError("x".into())),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    // variants without an explicit arm fall through to 500
    assert_eq!(
        status_of(SvmIndexerError::SerializationError("x".into())),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        status_of(SvmIndexerError::RpcRequestFailed("x".into())),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn db_error_builds_database_variant_without_id() {
    let err = SvmIndexerError::db_error("boom", DbErr::Custom("inner".into()));
    assert!(matches!(err, SvmIndexerError::DatabaseError(_, _, None)));
    assert_eq!(status_of(err), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn db_error_with_id_carries_the_element_id() {
    let err = SvmIndexerError::db_error_with_id("boom", DbErr::Custom("inner".into()), 7);
    assert!(matches!(err, SvmIndexerError::DatabaseError(_, _, Some(7))));
}

#[test]
fn dberr_converts_into_database_error() {
    let err: SvmIndexerError = DbErr::Custom("inner".into()).into();
    assert!(matches!(err, SvmIndexerError::DatabaseError(_, _, None)));
}

#[test]
fn only_the_no_block_codes_read_as_a_missing_block() {
    for code in [-32001, -32004, -32007, -32009] {
        assert!(
            SvmIndexerError::RpcResponseError(code, "no block".into()).is_block_missing(),
            "{code} is the node answering that it holds no block"
        );
    }
    // a failure that merely prevented an answer leaves the slot unknown; reading it as an empty
    // block would let a blip fabricate a reorg drop
    assert!(!SvmIndexerError::RpcResponseError(-32005, "unhealthy".into()).is_block_missing());
    assert!(!SvmIndexerError::RpcResponseError(-32012, "scan limit".into()).is_block_missing());
    assert!(!SvmIndexerError::RpcRequestFailed("timed out".into()).is_block_missing());
}

#[tokio::test]
async fn response_body_is_a_json_message_envelope() {
    let resp = SvmIndexerError::BadInput("bad thing".into()).into_response();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["message"], "Invalid Parameters! bad thing");
}

#[tokio::test]
async fn db_error_with_id_appends_id_but_hides_the_raw_error() {
    let resp = SvmIndexerError::db_error_with_id(
        "hidden internal detail",
        DbErr::Custom("inner".into()),
        42,
    )
    .into_response();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let message = body["message"].as_str().unwrap();
    assert!(
        message.contains("[id = 42]"),
        "the element id is surfaced to the client"
    );
    assert!(
        !message.contains("inner"),
        "the raw sea-orm error stays server-side"
    );
}
