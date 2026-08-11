use serde_json::json;

use svm_indexer::core::indexer_error::{IndexerErrorKind, record};

use crate::common::{pubkey, reset_db, seed_program, seed_rpc, server, test_db};

#[tokio::test]
async fn list_errors_orders_unseen_first_then_newest() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let db = test_db().await;
    record(&db, IndexerErrorKind::Database, "first".into(), None, None).await;
    record(&db, IndexerErrorKind::Scheduling, "second".into(), None, None).await;
    record(&db, IndexerErrorKind::Indexing, "third".into(), None, None).await;

    let res = server().authed_client().do_get("/admin/errors").await?;
    assert_eq!(res.status(), 200);
    let body = res.json_body_as::<serde_json::Value>()?;
    assert_eq!(body["total"], 3);
    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 3);
    assert_eq!(data[0]["message"], "third");
    assert_eq!(data[0]["kind"], "indexing");
    assert_eq!(data[2]["message"], "first");
    assert_eq!(data[2]["kind"], "database");
    Ok(())
}

#[tokio::test]
async fn list_errors_filters_by_seen() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let db = test_db().await;
    record(&db, IndexerErrorKind::Indexing, "one".into(), None, None).await;
    record(&db, IndexerErrorKind::Indexing, "two".into(), None, None).await;

    let listed = server().authed_client().do_get("/admin/errors").await?
        .json_body_as::<serde_json::Value>()?;
    let newest_id = listed["data"][0]["id"].as_i64().unwrap();
    let marked = server().authed_client()
        .do_patch(&format!("/admin/errors/{}", newest_id), json!({}))
        .await?;
    assert_eq!(marked.status(), 204);

    let seen = server().authed_client().do_get("/admin/errors?seen=true").await?
        .json_body_as::<serde_json::Value>()?;
    assert_eq!(seen["total"], 1);
    assert_eq!(seen["data"][0]["id"], newest_id);

    let unseen = server().authed_client().do_get("/admin/errors?seen=false").await?
        .json_body_as::<serde_json::Value>()?;
    assert_eq!(unseen["total"], 1);
    assert_eq!(unseen["data"][0]["seen"], false);
    Ok(())
}

#[tokio::test]
async fn record_persists_program_and_slot() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://errors-rpc.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;
    record(&db, IndexerErrorKind::Indexing, "boom".into(), Some(program_id), Some(42)).await;

    let body = server().authed_client().do_get("/admin/errors").await?
        .json_body_as::<serde_json::Value>()?;
    let err = &body["data"][0];
    assert_eq!(err["message"], "boom");
    assert_eq!(err["kind"], "indexing");
    assert_eq!(err["program_id"], program_id);
    assert_eq!(err["slot"], 42);
    assert_eq!(err["seen"], false);
    Ok(())
}

#[tokio::test]
async fn mark_unknown_error_seen_is_not_found() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let res = server().authed_client()
        .do_patch("/admin/errors/999999", json!({}))
        .await?;
    assert_eq!(res.status(), 404);
    Ok(())
}

#[tokio::test]
async fn errors_endpoints_require_auth() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let list = server().unauthed_client().do_get("/admin/errors").await?;
    assert_eq!(list.status(), 401);
    let mark = server().unauthed_client().do_patch("/admin/errors/1", json!({})).await?;
    assert_eq!(mark.status(), 401);
    Ok(())
}
