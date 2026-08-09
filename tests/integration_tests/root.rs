use crate::common::{reset_db, server};

#[tokio::test]
async fn health_check_returns_ok() -> httpc_test::Result<()> {
    let _guard = reset_db().await;

    let res = server().unauthed_client().do_get("/").await?;
    assert_eq!(res.status(), 200);

    let body = res.json_body_as::<serde_json::Value>()?;
    assert_eq!(body["status"], "ok");
    Ok(())
}

#[tokio::test]
async fn unknown_route_returns_404() -> httpc_test::Result<()> {
    let _guard = reset_db().await;

    let res = server().unauthed_client().do_get("/definitely-not-real").await?;
    assert_eq!(res.status(), 404);
    Ok(())
}
