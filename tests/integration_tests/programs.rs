use serde_json::json;

use crate::common::{
    assert_datetimes_equal, parse_datetime, pubkey, reset_db, seed_account, seed_event,
    seed_program, seed_rpc, seed_transaction, seed_transaction_account, server, signature, test_db,
};

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
    let _guard = reset_db().await;

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
    assert!(body["created_at"].is_string());
    assert!(body["updated_at"].is_string());
    assert_datetimes_equal(&body["created_at"], &body["updated_at"]);
    Ok(())
}

#[tokio::test]
async fn add_program_with_rpc_url_creates_rpc() -> httpc_test::Result<()> {
    let _guard = reset_db().await;

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
    let _guard = reset_db().await;

    let res = server()
        .authed_client()
        .do_post("/admin/programs", json!({"address": address(0)}))
        .await?;
    assert_eq!(res.status(), 400);
    Ok(())
}

#[tokio::test]
async fn add_program_invalid_address_is_bad_request() -> httpc_test::Result<()> {
    let _guard = reset_db().await;

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
    let _guard = reset_db().await;

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
    let _guard = reset_db().await;

    let rpc_id = create_rpc("https://program-dup.example.com").await;

    server()
        .authed_client()
        .do_post(
            "/admin/programs",
            json!({"rpc_id": rpc_id, "address": address(0)}),
        )
        .await?;

    let res = server()
        .authed_client()
        .do_post(
            "/admin/programs",
            json!({"rpc_id": rpc_id, "address": address(0)}),
        )
        .await?;
    assert_eq!(res.status(), 500);
    Ok(())
}

#[tokio::test]
async fn add_program_without_auth_is_unauthorized() -> httpc_test::Result<()> {
    let _guard = reset_db().await;

    let res = server()
        .unauthed_client()
        .do_post("/admin/programs", json!({"address": address(0)}))
        .await?;
    assert_eq!(res.status(), 401);
    Ok(())
}

#[tokio::test]
async fn list_programs_with_pagination() -> httpc_test::Result<()> {
    let _guard = reset_db().await;

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
    let _guard = reset_db().await;

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
    let _guard = reset_db().await;

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
    let created_body = created.json_body_as::<serde_json::Value>()?;
    let id = created_body["id"].as_u64().unwrap();
    let created_at = created_body["created_at"].clone();

    // Sleep so a stale updated_at would be detectable.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

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
    assert_eq!(body["created_at"], created_at);
    assert!(parse_datetime(&body["updated_at"]) > parse_datetime(&created_at));
    Ok(())
}

#[tokio::test]
async fn update_program_no_fields_is_bad_request() -> httpc_test::Result<()> {
    let _guard = reset_db().await;

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
    let _guard = reset_db().await;

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
    let _guard = reset_db().await;

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
async fn add_program_defaults_to_active() -> httpc_test::Result<()> {
    let _guard = reset_db().await;

    let rpc_id = create_rpc("https://program-active-default.example.com").await;
    let res = server()
        .authed_client()
        .do_post(
            "/admin/programs",
            json!({"rpc_id": rpc_id, "address": address(0)}),
        )
        .await?;
    let body = res.json_body_as::<serde_json::Value>()?;
    assert_eq!(
        body["active"], true,
        "a new program is indexed unless paused"
    );
    Ok(())
}

#[tokio::test]
async fn activate_and_deactivate_program_toggles_flag() -> httpc_test::Result<()> {
    let _guard = reset_db().await;

    let rpc_id = create_rpc("https://program-toggle.example.com").await;
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
        .do_post(&format!("/admin/programs/{}/deactivate", id), json!({}))
        .await?;
    assert_eq!(res.status(), 200);
    assert_eq!(res.json_body_as::<serde_json::Value>()?["active"], false);

    // the flag is durable — a subsequent read reflects the paused state
    let res = server()
        .authed_client()
        .do_get(&format!("/admin/programs/{}", id))
        .await?;
    assert_eq!(res.json_body_as::<serde_json::Value>()?["active"], false);

    let res = server()
        .authed_client()
        .do_post(&format!("/admin/programs/{}/activate", id), json!({}))
        .await?;
    assert_eq!(res.status(), 200);
    assert_eq!(res.json_body_as::<serde_json::Value>()?["active"], true);
    Ok(())
}

#[tokio::test]
async fn activate_unknown_program_is_not_found() -> httpc_test::Result<()> {
    let _guard = reset_db().await;

    let res = server()
        .authed_client()
        .do_post("/admin/programs/9999/activate", json!({}))
        .await?;
    assert_eq!(res.status(), 404);
    Ok(())
}

#[tokio::test]
async fn add_program_with_account_filters_persists() -> httpc_test::Result<()> {
    let _guard = reset_db().await;

    let rpc_id = create_rpc("https://program-filters.example.com").await;
    let res = server()
        .authed_client()
        .do_post(
            "/admin/programs",
            json!({
                "rpc_id": rpc_id,
                "address": address(0),
                "account_filters": {
                    "data_size": 165,
                    "memcmp": [{ "offset": 0, "bytes": address(3) }]
                }
            }),
        )
        .await?;
    assert_eq!(res.status(), 201);
    let id = res.json_body_as::<serde_json::Value>()?["id"]
        .as_u64()
        .unwrap();

    // durable — a fresh read returns the stored descriptor
    let res = server()
        .authed_client()
        .do_get(&format!("/admin/programs/{}", id))
        .await?;
    let body = res.json_body_as::<serde_json::Value>()?;
    assert_eq!(body["account_filters"]["data_size"], 165);
    assert_eq!(body["account_filters"]["memcmp"][0]["offset"], 0);
    Ok(())
}

#[tokio::test]
async fn add_program_rejects_invalid_account_filters() -> httpc_test::Result<()> {
    let _guard = reset_db().await;

    let rpc_id = create_rpc("https://program-bad-filters.example.com").await;
    let res = server()
        .authed_client()
        .do_post(
            "/admin/programs",
            json!({
                "rpc_id": rpc_id,
                "address": address(0),
                "account_filters": { "addresses": ["not-a-pubkey"] }
            }),
        )
        .await?;
    assert_eq!(res.status(), 400);
    Ok(())
}

#[tokio::test]
async fn update_program_clears_account_filters() -> httpc_test::Result<()> {
    let _guard = reset_db().await;

    let rpc_id = create_rpc("https://program-clear-filters.example.com").await;
    let created = server()
        .authed_client()
        .do_post(
            "/admin/programs",
            json!({
                "rpc_id": rpc_id,
                "address": address(0),
                "account_filters": { "data_size": 165 }
            }),
        )
        .await?;
    let id = created.json_body_as::<serde_json::Value>()?["id"]
        .as_u64()
        .unwrap();

    // an empty descriptor resets the program back to track-all
    let res = server()
        .authed_client()
        .do_patch(
            &format!("/admin/programs/{}", id),
            json!({"account_filters": {}}),
        )
        .await?;
    assert_eq!(res.status(), 200);
    assert!(res.json_body_as::<serde_json::Value>()?["account_filters"].is_null());

    let res = server()
        .authed_client()
        .do_get(&format!("/admin/programs/{}", id))
        .await?;
    assert!(res.json_body_as::<serde_json::Value>()?["account_filters"].is_null());
    Ok(())
}

#[tokio::test]
async fn update_program_rpc_resets_the_cursor_but_keeps_it_on_other_edits() -> httpc_test::Result<()>
{
    let _guard = reset_db().await;

    let rpc_id = create_rpc("https://program-repoint-from.example.com").await;
    let other_rpc_id = create_rpc("https://program-repoint-to.example.com").await;
    let created = server()
        .authed_client()
        .do_post(
            "/admin/programs",
            json!({"rpc_id": rpc_id, "address": address(0), "start_slot": 900}),
        )
        .await?;
    let id = created.json_body_as::<serde_json::Value>()?["id"]
        .as_u64()
        .unwrap();

    // naming the same rpc is not a repoint
    let res = server()
        .authed_client()
        .do_patch(
            &format!("/admin/programs/{}", id),
            json!({"rpc_id": rpc_id, "name": "same-network"}),
        )
        .await?;
    assert_eq!(res.status(), 200);
    assert_eq!(res.json_body_as::<serde_json::Value>()?["start_slot"], 900);

    // slots are not comparable across networks, so the cursor cannot follow the program
    let res = server()
        .authed_client()
        .do_patch(
            &format!("/admin/programs/{}", id),
            json!({"rpc_id": other_rpc_id}),
        )
        .await?;
    assert_eq!(res.status(), 200);
    let body = res.json_body_as::<serde_json::Value>()?;
    assert_eq!(body["rpc_id"], other_rpc_id);
    assert!(body["start_slot"].is_null());
    assert!(body["last_processed_slot"].is_null());
    assert!(body["last_processed_at"].is_null());
    Ok(())
}

#[tokio::test]
async fn update_program_rpc_drops_account_state_but_keeps_history() -> httpc_test::Result<()> {
    use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, sea_query::Expr};
    use svm_indexer::db::entities::{program_accounts, program_events, transactions};

    let _guard = reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://program-accounts-repoint-from.example.com").await;
    let other_rpc_id = seed_rpc(&db, "https://program-accounts-repoint-to.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(60)).await;
    let sig = signature(80);
    seed_account(
        &db,
        program_id,
        rpc_id,
        &pubkey(61),
        &pubkey(60),
        &[7],
        1_000,
        900,
        0,
        None,
    )
    .await;
    seed_transaction(&db, Some(program_id), rpc_id, &sig, 900, true, None).await;
    seed_event(&db, program_id, rpc_id, &sig, 0, 900, &[9], None).await;

    // only a linked event exercises the RESTRICT that guards program_accounts
    let account_id = program_accounts::Entity::find()
        .filter(program_accounts::Column::ProgramId.eq(program_id))
        .one(&db)
        .await
        .unwrap()
        .unwrap()
        .id;
    program_events::Entity::update_many()
        .col_expr(
            program_events::Column::ProgramAccountId,
            Expr::value(Some(account_id)),
        )
        .filter(program_events::Column::ProgramId.eq(program_id))
        .exec(&db)
        .await
        .unwrap();

    let res = server()
        .authed_client()
        .do_patch(
            &format!("/admin/programs/{}", program_id),
            json!({"rpc_id": other_rpc_id}),
        )
        .await?;
    assert_eq!(res.status(), 200);

    assert_eq!(
        program_accounts::Entity::find()
            .filter(program_accounts::Column::ProgramId.eq(program_id))
            .count(&db)
            .await
            .unwrap(),
        0,
        "a snapshot at the old chain's slot would reject every write from the new one"
    );
    assert_eq!(
        program_events::Entity::find()
            .filter(program_events::Column::ProgramId.eq(program_id))
            .count(&db)
            .await
            .unwrap(),
        1,
        "events are history and stay"
    );
    assert_eq!(
        transactions::Entity::find()
            .filter(transactions::Column::ProgramId.eq(program_id))
            .count(&db)
            .await
            .unwrap(),
        1
    );
    Ok(())
}

#[tokio::test]
async fn update_program_rpc_clears_the_backfill_frontier() -> httpc_test::Result<()> {
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, sea_query::Expr};
    use svm_indexer::db::entities::programs;

    let _guard = reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://backfill-frontier-from.example.com").await;
    let other_rpc_id = seed_rpc(&db, "https://backfill-frontier-to.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(62)).await;

    programs::Entity::update_many()
        .col_expr(programs::Column::BackfillSlot, Expr::value(900_i64))
        .col_expr(
            programs::Column::BackfillCursor,
            Expr::value(signature(81).as_ref().to_vec()),
        )
        .filter(programs::Column::Id.eq(program_id))
        .exec(&db)
        .await
        .unwrap();

    let res = server()
        .authed_client()
        .do_patch(
            &format!("/admin/programs/{}", program_id),
            json!({"rpc_id": other_rpc_id}),
        )
        .await?;
    assert_eq!(res.status(), 200);

    let program = programs::Entity::find_by_id(program_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        program.backfill_slot, None,
        "the frontier is a position on the old chain"
    );
    assert_eq!(program.backfill_cursor, None);
    Ok(())
}

#[tokio::test]
async fn delete_program_not_found() -> httpc_test::Result<()> {
    let _guard = reset_db().await;

    let res = server()
        .authed_client()
        .do_delete("/admin/programs/9999")
        .await?;
    assert_eq!(res.status(), 404);
    Ok(())
}

/// A program holding one row in every table that references it.
async fn seed_indexed_program(db: &sea_orm::DatabaseConnection, http_url: &str) -> i32 {
    let rpc_id = seed_rpc(db, http_url).await;
    let program_id = seed_program(db, rpc_id, &pubkey(50)).await;
    let sig = signature(70);
    seed_account(
        db,
        program_id,
        rpc_id,
        &pubkey(51),
        &pubkey(50),
        &[1, 2, 3],
        1_000,
        700,
        0,
        None,
    )
    .await;
    seed_transaction(db, Some(program_id), rpc_id, &sig, 700, true, None).await;
    seed_transaction_account(db, &sig, &pubkey(52), 700, true).await;
    seed_event(db, program_id, rpc_id, &sig, 0, 700, &[9], None).await;
    program_id
}

#[tokio::test]
async fn delete_program_with_indexed_data_is_a_conflict() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let db = test_db().await;
    let program_id = seed_indexed_program(&db, "https://program-delete-blocked.example.com").await;

    let res = server()
        .authed_client()
        .do_delete(&format!("/admin/programs/{}", program_id))
        .await?;
    assert_eq!(res.status(), 409);

    // the restriction held, so the program and its data are still there
    let res = server()
        .authed_client()
        .do_get(&format!("/admin/programs/{}", program_id))
        .await?;
    assert_eq!(res.status(), 200);
    Ok(())
}

#[tokio::test]
async fn delete_program_with_force_purges_its_data() -> httpc_test::Result<()> {
    use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};
    use svm_indexer::db::entities::{
        program_accounts, program_events, transaction_accounts, transactions,
    };

    let _guard = reset_db().await;
    let db = test_db().await;
    let program_id = seed_indexed_program(&db, "https://program-delete-force.example.com").await;

    let res = server()
        .authed_client()
        .do_delete(&format!("/admin/programs/{}?force=true", program_id))
        .await?;
    assert_eq!(res.status(), 204);

    let res = server()
        .authed_client()
        .do_get(&format!("/admin/programs/{}", program_id))
        .await?;
    assert_eq!(res.status(), 404);

    assert_eq!(
        program_accounts::Entity::find()
            .filter(program_accounts::Column::ProgramId.eq(program_id))
            .count(&db)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        program_events::Entity::find()
            .filter(program_events::Column::ProgramId.eq(program_id))
            .count(&db)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        transactions::Entity::find()
            .filter(transactions::Column::ProgramId.eq(program_id))
            .count(&db)
            .await
            .unwrap(),
        0
    );
    // no foreign key covers these, so the purge has to reach them by signature
    assert_eq!(
        transaction_accounts::Entity::find()
            .filter(transaction_accounts::Column::TxSignature.eq(signature(70).as_ref().to_vec()))
            .count(&db)
            .await
            .unwrap(),
        0
    );
    Ok(())
}

#[tokio::test]
async fn add_program_with_addresses_allowlist_persists() -> httpc_test::Result<()> {
    let _guard = reset_db().await;

    let rpc_id = create_rpc("https://program-allowlist.example.com").await;
    let res = server()
        .authed_client()
        .do_post(
            "/admin/programs",
            json!({
                "rpc_id": rpc_id,
                "address": address(0),
                "account_filters": { "addresses": [address(3), address(4)] }
            }),
        )
        .await?;
    assert_eq!(res.status(), 201);
    let id = res.json_body_as::<serde_json::Value>()?["id"]
        .as_u64()
        .unwrap();

    let body = server()
        .authed_client()
        .do_get(&format!("/admin/programs/{}", id))
        .await?
        .json_body_as::<serde_json::Value>()?;
    let addresses = body["account_filters"]["addresses"].as_array().unwrap();
    assert_eq!(addresses.len(), 2);
    assert_eq!(addresses[0], address(3));
    Ok(())
}

#[tokio::test]
async fn add_program_rejects_invalid_memcmp_bytes() -> httpc_test::Result<()> {
    let _guard = reset_db().await;

    let rpc_id = create_rpc("https://program-bad-memcmp.example.com").await;
    let res = server()
        .authed_client()
        .do_post(
            "/admin/programs",
            json!({
                "rpc_id": rpc_id,
                "address": address(0),
                "account_filters": { "memcmp": [{ "offset": 0, "bytes": "0OIl" }] }
            }),
        )
        .await?;
    assert_eq!(res.status(), 400);
    Ok(())
}

#[tokio::test]
async fn deactivate_unknown_program_is_not_found() -> httpc_test::Result<()> {
    let _guard = reset_db().await;

    let res = server()
        .authed_client()
        .do_post("/admin/programs/9999/deactivate", json!({}))
        .await?;
    assert_eq!(res.status(), 404);
    Ok(())
}

#[tokio::test]
async fn activation_endpoints_require_auth() -> httpc_test::Result<()> {
    let _guard = reset_db().await;

    let activate = server()
        .unauthed_client()
        .do_post("/admin/programs/1/activate", json!({}))
        .await?;
    assert_eq!(activate.status(), 401);

    let deactivate = server()
        .unauthed_client()
        .do_post("/admin/programs/1/deactivate", json!({}))
        .await?;
    assert_eq!(deactivate.status(), 401);
    Ok(())
}
