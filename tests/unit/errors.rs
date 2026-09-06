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
fn each_no_block_code_reads_as_its_own_cause() {
    let pruned = SvmIndexerError::RpcResponseError(-32001, "cleaned up".into());
    assert!(pruned.is_block_pruned());
    assert!(!pruned.is_slot_skipped() && !pruned.is_block_not_yet_available());

    let too_early = SvmIndexerError::RpcResponseError(-32004, "not available".into());
    assert!(too_early.is_block_not_yet_available());
    assert!(!too_early.is_slot_skipped() && !too_early.is_block_pruned());

    // the local blockstore and the archival backend answer with different codes for the same fact
    for code in [-32007, -32009] {
        let skipped = SvmIndexerError::RpcResponseError(code, "was skipped".into());
        assert!(skipped.is_slot_skipped(), "{code} is no block on the chain");
        assert!(!skipped.is_block_pruned() && !skipped.is_block_not_yet_available());
    }
}

#[test]
fn an_unlisted_failure_reads_as_no_cause_at_all() {
    // a failure that merely prevented an answer leaves the slot unknown; matching it to a cause —
    // `is_slot_skipped` above all — would let a blip fabricate a reorg drop
    for err in [
        SvmIndexerError::RpcResponseError(-32005, "unhealthy".into()),
        SvmIndexerError::RpcResponseError(-32012, "scan limit".into()),
        SvmIndexerError::RpcResponseError(-32603, "internal".into()),
        SvmIndexerError::RpcRequestFailed("timed out".into()),
    ] {
        assert!(
            !err.is_slot_skipped() && !err.is_block_pruned() && !err.is_block_not_yet_available(),
            "{err} says the node could not answer, not that the block is absent"
        );
    }
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
