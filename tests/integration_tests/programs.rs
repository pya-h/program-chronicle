use serde_json::json;

use crate::common::{reset_db, server};

fn address(idx: usize) -> &'static str {
    const ADDRESSES: &[&str] = &[
        "4GYvtbs7da26tLaZt9PNQWLesq2riwEN6fi9tGF91A5P",
        "9oFYVPXLVyv3YchXnULcVuwGcTggK5nC4VkqjyRmSuAx",
        "11111111111111111111111111111111",
        "So11111111111111111111111111111111111111112",
        "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
        "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL",
        "SysvarC1ock11111111111111111111111111111111",
        "Vote111111111111111111111111111111111111111",
    ];
    ADDRESSES[idx % ADDRESSES.len()]
}

async fn create_rpc(http_url: &str) -> u64 {
    server()
        .authed_client()
        .do_post(
            "/admin/rpcs",
            json!({
                "http_url": http_url,
                "ws_url": format!("wss://{}", http_url.trim_start_matches("https://"))
            }),
        )
        .await
        .unwrap()
        .json_body_as::<serde_json::Value>()
        .unwrap()["id"]
        .as_u64()
        .unwrap()
}

#[tokio::test]
async fn add_program_with_rpc_id_happy() -> httpc_test::Result<()> {
    reset_db().await;

    let rpc_id = create_rpc("https://program-rpc.example.com").await;

    let res = server()
        .authed_client()
        .do_post(
            "/admin/programs",
            json!({
                "rpc_id": rpc_id,
                "name": "test-program",
                "address": address(0)
            }),
        )
        .await?;
    assert_eq!(res.status(), 201);

    let body = res.json_body_as::<serde_json::Value>()?;
    assert_eq!(body["name"], "test-program");
    assert_eq!(body["rpc_id"], rpc_id);
    Ok(())
}

#[tokio::test]
async fn add_program_with_rpc_url_creates_rpc() -> httpc_test::Result<()> {
    reset_db().await;

    let res = server()
        .authed_client()
        .do_post(
            "/admin/programs",
            json!({
                "rpc_url": {
                    "http_url": "https://new-rpc-for-program.example.com",
                    "ws_url": "wss://new-rpc-for-program.example.com"
                },
                "address": address(1)
            }),
        )
        .await?;
    assert_eq!(res.status(), 201);

    let body = res.json_body_as::<serde_json::Value>()?;
    assert!(body["rpc"]["id"].is_number());
    assert_eq!(
        body["rpc"]["http_url"],
        "https://new-rpc-for-program.example.com"
    );
    Ok(())
}

#[tokio::test]
async fn add_program_without_rpc_is_bad_request() -> httpc_test::Result<()> {
    reset_db().await;

    let res = server()
        .authed_client()
        .do_post("/admin/programs", json!({"address": address(0)}))
        .await?;
    assert_eq!(res.status(), 400);
    Ok(())
}

#[tokio::test]
async fn add_program_invalid_address_is_bad_request() -> httpc_test::Result<()> {
    reset_db().await;

    let rpc_id = create_rpc("https://program-rpc-invalid.example.com").await;

    let res = server()
        .authed_client()
        .do_post(
            "/admin/programs",
            json!({"rpc_id": rpc_id, "address": "not-a-valid-address"}),
        )
        .await?;
    assert_eq!(res.status(), 400);
    Ok(())
}

#[tokio::test]
async fn add_program_rpc_id_not_found() -> httpc_test::Result<()> {
    reset_db().await;

    let res = server()
        .authed_client()
        .do_post(
            "/admin/programs",
            json!({"rpc_id": 9999, "address": address(0)}),
        )
        .await?;
    assert_eq!(res.status(), 404);
    Ok(())
}

#[tokio::test]
async fn add_program_duplicate_address_on_same_rpc_fails() -> httpc_test::Result<()> {
    reset_db().await;

    let rpc_id = create_rpc("https://program-dup.example.com").await;

    server()
        .authed_client()
        .do_post("/admin/programs", json!({"rpc_id": rpc_id, "address": address(0)}))
        .await?;

    let res = server()
        .authed_client()
        .do_post("/admin/programs", json!({"rpc_id": rpc_id, "address": address(0)}))
        .await?;
    assert_eq!(res.status(), 500);
    Ok(())
}

#[tokio::test]
async fn add_program_without_auth_is_unauthorized() -> httpc_test::Result<()> {
    reset_db().await;

    let res = server()
        .unauthed_client()
        .do_post("/admin/programs", json!({"address": address(0)}))
        .await?;
    assert_eq!(res.status(), 401);
    Ok(())
}

#[tokio::test]
async fn list_programs_with_pagination() -> httpc_test::Result<()> {
    reset_db().await;

    let rpc_id = create_rpc("https://program-list.example.com").await;
    for i in 0..5 {
        server()
            .authed_client()
            .do_post(
                "/admin/programs",
                json!({"rpc_id": rpc_id, "address": address(i)}),
            )
            .await?;
    }

    let res = server()
        .authed_client()
        .do_get("/admin/programs?page=1&per_page=2")
        .await?;
    assert_eq!(res.status(), 200);

    let body = res.json_body_as::<serde_json::Value>()?;
    assert_eq!(body["total"], 5);
    assert_eq!(body["pages"], 3);
    assert_eq!(body["data"].as_array().unwrap().len(), 2);
    Ok(())
}

#[tokio::test]
async fn get_single_program_and_not_found() -> httpc_test::Result<()> {
    reset_db().await;

    let rpc_id = create_rpc("https://program-single.example.com").await;
    let created = server()
        .authed_client()
        .do_post(
            "/admin/programs",
            json!({"rpc_id": rpc_id, "address": address(0)}),
        )
        .await?;
    let id = created.json_body_as::<serde_json::Value>()?["id"]
        .as_u64()
        .unwrap();

    let res = server()
        .authed_client()
        .do_get(&format!("/admin/programs/{}", id))
        .await?;
    assert_eq!(res.status(), 200);

    let res = server()
        .authed_client()
        .do_get("/admin/programs/9999")
        .await?;
    assert_eq!(res.status(), 404);
    Ok(())
}

#[tokio::test]
async fn update_program_happy() -> httpc_test::Result<()> {
    reset_db().await;

    let rpc_id = create_rpc("https://program-update.example.com").await;
    let created = server()
        .authed_client()
        .do_post(
            "/admin/programs",
            json!({
                "rpc_id": rpc_id,
                "name": "old-name",
                "address": address(0)
            }),
        )
        .await?;
    let id = created.json_body_as::<serde_json::Value>()?["id"]
        .as_u64()
        .unwrap();

    let res = server()
        .authed_client()
        .do_patch(
            &format!("/admin/programs/{}", id),
            json!({"name": "new-name"}),
        )
        .await?;
    assert_eq!(res.status(), 200);

    let body = res.json_body_as::<serde_json::Value>()?;
    assert_eq!(body["name"], "new-name");
    Ok(())
}

#[tokio::test]
async fn update_program_no_fields_is_bad_request() -> httpc_test::Result<()> {
    reset_db().await;

    let rpc_id = create_rpc("https://program-update-bad.example.com").await;
    let created = server()
        .authed_client()
        .do_post(
            "/admin/programs",
            json!({"rpc_id": rpc_id, "address": address(0)}),
        )
        .await?;
    let id = created.json_body_as::<serde_json::Value>()?["id"]
        .as_u64()
        .unwrap();

    let res = server()
        .authed_client()
        .do_patch(&format!("/admin/programs/{}", id), json!({}))
        .await?;
    assert_eq!(res.status(), 400);
    Ok(())
}

#[tokio::test]
async fn update_program_invalid_address_is_bad_request() -> httpc_test::Result<()> {
    reset_db().await;

    let rpc_id = create_rpc("https://program-update-addr.example.com").await;
    let created = server()
        .authed_client()
        .do_post(
            "/admin/programs",
            json!({"rpc_id": rpc_id, "address": address(0)}),
        )
        .await?;
    let id = created.json_body_as::<serde_json::Value>()?["id"]
        .as_u64()
        .unwrap();

    let res = server()
        .authed_client()
        .do_patch(
            &format!("/admin/programs/{}", id),
            json!({"address": "bad-address"}),
        )
        .await?;
    assert_eq!(res.status(), 400);
    Ok(())
}

#[tokio::test]
async fn delete_program_happy() -> httpc_test::Result<()> {
    reset_db().await;

    let rpc_id = create_rpc("https://program-delete.example.com").await;
    let created = server()
        .authed_client()
        .do_post(
            "/admin/programs",
            json!({"rpc_id": rpc_id, "address": address(0)}),
        )
        .await?;
    let id = created.json_body_as::<serde_json::Value>()?["id"]
        .as_u64()
        .unwrap();

    let res = server()
        .authed_client()
        .do_delete(&format!("/admin/programs/{}", id))
        .await?;
    assert_eq!(res.status(), 204);

    let res = server()
        .authed_client()
        .do_get(&format!("/admin/programs/{}", id))
        .await?;
    assert_eq!(res.status(), 404);
    Ok(())
}

#[tokio::test]
async fn delete_program_not_found() -> httpc_test::Result<()> {
    reset_db().await;

    let res = server()
        .authed_client()
        .do_delete("/admin/programs/9999")
        .await?;
    assert_eq!(res.status(), 404);
    Ok(())
}
