use crate::common::{reset_db, server};

#[tokio::test]
async fn validate_admin_passkey_ok() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let res = server().authed_client().do_get("/admin").await?;
    assert_eq!(res.status(), 200);
    Ok(())
}

#[tokio::test]
async fn validate_admin_requires_auth() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let res = server().unauthed_client().do_get("/admin").await?;
    assert_eq!(res.status(), 401);
    Ok(())
}

#[tokio::test]
async fn a_wrong_passkey_is_rejected() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let srv = server();
    let res = reqwest::Client::new()
        .get(format!("{}/admin", srv.base_url))
        .header("x-security", "definitely-not-the-passkey")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
    Ok(())
}
