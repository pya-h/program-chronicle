use serde_json::json;

use crate::common::{reset_db, setup};

#[tokio::test]
async fn add_rpc_happy_with_both_urls() -> httpc_test::Result<()> {
    let state = setup().await.lock().await;
    reset_db(&state).await;

    let res = state
        .authed_client
        .do_post(
            "/admin/rpcs",
            json!({
                "http_url": "https://rpc-both.example.com",
                "ws_url": "wss://rpc-both.example.com"
            }),
        )
        .await?;
    assert_eq!(res.status(), 201);

    let body = res.json_body_as::<serde_json::Value>()?;
    assert_eq!(body["http_url"], "https://rpc-both.example.com");
    assert_eq!(body["ws_url"], "wss://rpc-both.example.com");
    assert!(body["id"].is_number());
    Ok(())
}

#[tokio::test]
async fn add_rpc_without_ws_url_auto_generates() -> httpc_test::Result<()> {
    let state = setup().await.lock().await;
    reset_db(&state).await;

    let res = state
        .authed_client
        .do_post("/admin/rpcs", json!({"http_url": "https://rpc-auto.example.com"}))
        .await?;
    assert_eq!(res.status(), 201);

    let body = res.json_body_as::<serde_json::Value>()?;
    assert_eq!(body["http_url"], "https://rpc-auto.example.com");
    assert_eq!(body["ws_url"], "wss://rpc-auto.example.com");
    Ok(())
}

#[tokio::test]
async fn add_rpc_missing_http_url_is_bad_request() -> httpc_test::Result<()> {
    let state = setup().await.lock().await;
    reset_db(&state).await;

    let res = state
        .authed_client
        .do_post("/admin/rpcs", json!({"ws_url": "wss://rpc.example.com"}))
        .await?;
    assert_eq!(res.status(), 400);
    Ok(())
}

#[tokio::test]
async fn add_rpc_invalid_url_is_bad_request() -> httpc_test::Result<()> {
    let state = setup().await.lock().await;
    reset_db(&state).await;

    let res = state
        .authed_client
        .do_post("/admin/rpcs", json!({"http_url": "not-a-url"}))
        .await?;
    assert_eq!(res.status(), 400);
    Ok(())
}

#[tokio::test]
async fn add_rpc_without_auth_is_unauthorized() -> httpc_test::Result<()> {
    let state = setup().await.lock().await;
    reset_db(&state).await;

    let res = state
        .unauthed_client
        .do_post("/admin/rpcs", json!({"http_url": "https://rpc.example.com"}))
        .await?;
    assert_eq!(res.status(), 401);
    Ok(())
}

#[tokio::test]
async fn add_rpc_duplicate_http_url_fails() -> httpc_test::Result<()> {
    let state = setup().await.lock().await;
    reset_db(&state).await;

    state
        .authed_client
        .do_post("/admin/rpcs", json!({"http_url": "https://rpc-dup.example.com"}))
        .await?;

    let res = state
        .authed_client
        .do_post("/admin/rpcs", json!({"http_url": "https://rpc-dup.example.com"}))
        .await?;
    assert_eq!(res.status(), 500);
    Ok(())
}

#[tokio::test]
async fn list_rpcs_with_pagination() -> httpc_test::Result<()> {
    let state = setup().await.lock().await;
    reset_db(&state).await;

    for i in 0..5 {
        state
            .authed_client
            .do_post(
                "/admin/rpcs",
                json!({
                    "http_url": format!("https://rpc-{}.example.com", i),
                    "ws_url": format!("wss://rpc-{}.example.com", i)
                }),
            )
            .await?;
    }

    let res = state
        .authed_client
        .do_get("/admin/rpcs?page=1&per_page=2")
        .await?;
    assert_eq!(res.status(), 200);

    let body = res.json_body_as::<serde_json::Value>()?;
    assert_eq!(body["total"], 5);
    assert_eq!(body["pages"], 3);
    assert_eq!(body["data"].as_array().unwrap().len(), 2);
    Ok(())
}

#[tokio::test]
async fn list_rpcs_without_auth_is_unauthorized() -> httpc_test::Result<()> {
    let state = setup().await.lock().await;
    reset_db(&state).await;

    let res = state.unauthed_client.do_get("/admin/rpcs").await?;
    assert_eq!(res.status(), 401);
    Ok(())
}

#[tokio::test]
async fn get_single_rpc_and_not_found() -> httpc_test::Result<()> {
    let state = setup().await.lock().await;
    reset_db(&state).await;

    let created = state
        .authed_client
        .do_post(
            "/admin/rpcs",
            json!({
                "http_url": "https://rpc-single.example.com",
                "ws_url": "wss://rpc-single.example.com"
            }),
        )
        .await?;
    let id = created.json_body_as::<serde_json::Value>()?["id"]
        .as_u64()
        .unwrap();

    let res = state
        .authed_client
        .do_get(&format!("/admin/rpcs/{}", id))
        .await?;
    assert_eq!(res.status(), 200);

    let res = state.authed_client.do_get("/admin/rpcs/9999").await?;
    assert_eq!(res.status(), 404);
    Ok(())
}

#[tokio::test]
async fn update_rpc_happy() -> httpc_test::Result<()> {
    let state = setup().await.lock().await;
    reset_db(&state).await;

    let created = state
        .authed_client
        .do_post(
            "/admin/rpcs",
            json!({
                "http_url": "https://rpc-update.example.com",
                "ws_url": "wss://rpc-update.example.com"
            }),
        )
        .await?;
    let id = created.json_body_as::<serde_json::Value>()?["id"]
        .as_u64()
        .unwrap();

    let res = state
        .authed_client
        .do_patch(
            &format!("/admin/rpcs/{}", id),
            json!({"http_url": "https://rpc-updated.example.com"}),
        )
        .await?;
    assert_eq!(res.status(), 200);

    let body = res.json_body_as::<serde_json::Value>()?;
    assert_eq!(body["http_url"], "https://rpc-updated.example.com");
    Ok(())
}

#[tokio::test]
async fn update_rpc_no_fields_is_bad_request() -> httpc_test::Result<()> {
    let state = setup().await.lock().await;
    reset_db(&state).await;

    let created = state
        .authed_client
        .do_post(
            "/admin/rpcs",
            json!({
                "http_url": "https://rpc-update-bad.example.com",
                "ws_url": "wss://rpc-update-bad.example.com"
            }),
        )
        .await?;
    let id = created.json_body_as::<serde_json::Value>()?["id"]
        .as_u64()
        .unwrap();

    let res = state
        .authed_client
        .do_patch(&format!("/admin/rpcs/{}", id), json!({}))
        .await?;
    assert_eq!(res.status(), 400);
    Ok(())
}

#[tokio::test]
async fn update_rpc_not_found() -> httpc_test::Result<()> {
    let state = setup().await.lock().await;
    reset_db(&state).await;

    let res = state
        .authed_client
        .do_patch("/admin/rpcs/9999", json!({"http_url": "https://x.com"}))
        .await?;
    assert_eq!(res.status(), 404);
    Ok(())
}

#[tokio::test]
async fn delete_rpc_happy() -> httpc_test::Result<()> {
    let state = setup().await.lock().await;
    reset_db(&state).await;

    let created = state
        .authed_client
        .do_post(
            "/admin/rpcs",
            json!({
                "http_url": "https://rpc-delete.example.com",
                "ws_url": "wss://rpc-delete.example.com"
            }),
        )
        .await?;
    let id = created.json_body_as::<serde_json::Value>()?["id"]
        .as_u64()
        .unwrap();

    let res = state
        .authed_client
        .do_delete(&format!("/admin/rpcs/{}", id))
        .await?;
    assert_eq!(res.status(), 204);

    let res = state
        .authed_client
        .do_get(&format!("/admin/rpcs/{}", id))
        .await?;
    assert_eq!(res.status(), 404);
    Ok(())
}

#[tokio::test]
async fn delete_rpc_not_found() -> httpc_test::Result<()> {
    let state = setup().await.lock().await;
    reset_db(&state).await;

    let res = state.authed_client.do_delete("/admin/rpcs/9999").await?;
    assert_eq!(res.status(), 404);
    Ok(())
}

#[tokio::test]
async fn delete_rpc_with_programs_is_blocked() -> httpc_test::Result<()> {
    let state = setup().await.lock().await;
    reset_db(&state).await;

    let created = state
        .authed_client
        .do_post(
            "/admin/rpcs",
            json!({
                "http_url": "https://rpc-protected.example.com",
                "ws_url": "wss://rpc-protected.example.com"
            }),
        )
        .await?;
    let rpc_id = created.json_body_as::<serde_json::Value>()?["id"]
        .as_u64()
        .unwrap();

    state
        .authed_client
        .do_post(
            "/admin/programs",
            json!({
                "rpc_id": rpc_id,
                "address": "4GYvtbs7da26tLaZt9PNQWLesq2riwEN6fi9tGF91A5P"
            }),
        )
        .await?;

    let res = state
        .authed_client
        .do_delete(&format!("/admin/rpcs/{}", rpc_id))
        .await?;
    assert_eq!(res.status(), 500);
    Ok(())
}
