use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::Value;

use crate::common::{
    pubkey, reset_db, seed_account, seed_event, seed_program, seed_rpc, seed_transaction, server,
    signature, test_db,
};

fn b64(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

// ---------- /accounts ----------

#[tokio::test]
async fn get_account_returns_latest_state() -> httpc_test::Result<()> {
    reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://get-account.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    let address = pubkey(10);
    let owner = pubkey(20);
    seed_account(&db, program_id, rpc_id, &address, &owner, &[9, 8, 7], 42, 500, 0, Some(1000)).await;
    // a newer state, which should be the one served
    seed_account(&db, program_id, rpc_id, &address, &owner, &[1, 1, 1], 43, 600, 0, Some(1001)).await;

    let res = server()
        .unauthed_client()
        .do_get(&format!("/accounts/{}", address))
        .await?;
    assert_eq!(res.status(), 200);

    let body = res.json_body_as::<Value>()?;
    assert_eq!(body["address"], address.to_string());
    assert_eq!(body["owner"], owner.to_string());
    assert_eq!(body["data"], b64(&[1, 1, 1]));
    assert_eq!(body["lamports"], 43);
    assert_eq!(body["slot"], 600);
    assert_eq!(body["block_time"], 1001);
    Ok(())
}

#[tokio::test]
async fn get_account_unknown_is_not_found() -> httpc_test::Result<()> {
    reset_db().await;
    let res = server()
        .unauthed_client()
        .do_get(&format!("/accounts/{}", pubkey(99)))
        .await?;
    assert_eq!(res.status(), 404);
    Ok(())
}

#[tokio::test]
async fn get_account_invalid_address_is_bad_request() -> httpc_test::Result<()> {
    reset_db().await;
    let res = server()
        .unauthed_client()
        .do_get("/accounts/not-a-real-address")
        .await?;
    assert_eq!(res.status(), 400);
    Ok(())
}

#[tokio::test]
async fn get_multiple_accounts_is_positional_with_nulls() -> httpc_test::Result<()> {
    reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://multi-account.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    let a = pubkey(10);
    let c = pubkey(12);
    seed_account(&db, program_id, rpc_id, &a, &pubkey(20), &[1], 10, 500, 0, Some(1000)).await;
    seed_account(&db, program_id, rpc_id, &c, &pubkey(22), &[3], 30, 500, 0, Some(1000)).await;

    // b (pubkey(11)) is not indexed → its slot stays null, order preserved
    let b = pubkey(11);
    let res = server()
        .unauthed_client()
        .do_get(&format!("/accounts?addresses={},{},{}", a, b, c))
        .await?;
    assert_eq!(res.status(), 200);

    let body = res.json_body_as::<Value>()?;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0]["address"], a.to_string());
    assert!(arr[1].is_null());
    assert_eq!(arr[2]["address"], c.to_string());
    Ok(())
}

#[tokio::test]
async fn get_multiple_accounts_empty_query_is_bad_request() -> httpc_test::Result<()> {
    reset_db().await;
    let res = server()
        .unauthed_client()
        .do_get("/accounts?addresses=%20")
        .await?;
    assert_eq!(res.status(), 400);
    Ok(())
}

#[tokio::test]
async fn get_account_history_with_range_and_pagination() -> httpc_test::Result<()> {
    reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://history.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    let address = pubkey(10);
    let owner = pubkey(20);
    for (i, slot) in [500_u64, 600, 700, 800].into_iter().enumerate() {
        seed_account(&db, program_id, rpc_id, &address, &owner, &[i as u8], 10, slot, 0, Some(1000 + slot as i64)).await;
    }

    // full history, newest first
    let res = server()
        .unauthed_client()
        .do_get(&format!("/accounts/{}/history", address))
        .await?;
    assert_eq!(res.status(), 200);
    let body = res.json_body_as::<Value>()?;
    assert_eq!(body["total"], 4);
    assert_eq!(body["data"][0]["slot"], 800);

    // bounded range [600, 700]
    let res = server()
        .unauthed_client()
        .do_get(&format!("/accounts/{}/history?from_slot=600&to_slot=700", address))
        .await?;
    let body = res.json_body_as::<Value>()?;
    assert_eq!(body["total"], 2);
    let slots: Vec<i64> = body["data"].as_array().unwrap().iter().map(|r| r["slot"].as_i64().unwrap()).collect();
    assert_eq!(slots, vec![700, 600]);

    // since_slot lower bound
    let res = server()
        .unauthed_client()
        .do_get(&format!("/accounts/{}/history?since_slot=700", address))
        .await?;
    let body = res.json_body_as::<Value>()?;
    assert_eq!(body["total"], 2);

    // pagination
    let res = server()
        .unauthed_client()
        .do_get(&format!("/accounts/{}/history?page=1&per_page=2", address))
        .await?;
    let body = res.json_body_as::<Value>()?;
    assert_eq!(body["total"], 4);
    assert_eq!(body["pages"], 2);
    assert_eq!(body["data"].as_array().unwrap().len(), 2);
    Ok(())
}

#[tokio::test]
async fn get_account_history_unknown_is_not_found() -> httpc_test::Result<()> {
    reset_db().await;
    let res = server()
        .unauthed_client()
        .do_get(&format!("/accounts/{}/history", pubkey(98)))
        .await?;
    assert_eq!(res.status(), 404);
    Ok(())
}

// ---------- /programs/{id}/accounts ----------

#[tokio::test]
async fn program_accounts_memcmp_and_data_size_filters() -> httpc_test::Result<()> {
    reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://gpa.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    let a = pubkey(10);
    let b = pubkey(11);
    seed_account(&db, program_id, rpc_id, &a, &pubkey(20), &[0xAA, 0xBB, 0, 0, 0, 0, 0, 0], 10, 500, 0, Some(1000)).await;
    seed_account(&db, program_id, rpc_id, &b, &pubkey(21), &[1, 2, 3, 4], 20, 501, 0, Some(1001)).await;

    // all accounts of the program
    let res = server()
        .unauthed_client()
        .do_get(&format!("/programs/{}/accounts", program_id))
        .await?;
    assert_eq!(res.status(), 200);
    assert_eq!(res.json_body_as::<Value>()?["total"], 2);

    // memcmp at offset 0 matches only account a
    let prefix = bs58::encode([0xAA_u8, 0xBB]).into_string();
    let res = server()
        .unauthed_client()
        .do_get(&format!("/programs/{}/accounts?memcmp=0:{}", program_id, prefix))
        .await?;
    let body = res.json_body_as::<Value>()?;
    assert_eq!(body["total"], 1);
    assert_eq!(body["data"][0]["address"], a.to_string());

    // dataSize matches only account b
    let res = server()
        .unauthed_client()
        .do_get(&format!("/programs/{}/accounts?dataSize=4", program_id))
        .await?;
    let body = res.json_body_as::<Value>()?;
    assert_eq!(body["total"], 1);
    assert_eq!(body["data"][0]["address"], b.to_string());

    // pagination
    let res = server()
        .unauthed_client()
        .do_get(&format!("/programs/{}/accounts?page=1&per_page=1", program_id))
        .await?;
    let body = res.json_body_as::<Value>()?;
    assert_eq!(body["total"], 2);
    assert_eq!(body["pages"], 2);
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
    Ok(())
}

#[tokio::test]
async fn program_accounts_malformed_memcmp_is_bad_request() -> httpc_test::Result<()> {
    reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://gpa-bad.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    let res = server()
        .unauthed_client()
        .do_get(&format!("/programs/{}/accounts?memcmp=not-an-offset", program_id))
        .await?;
    assert_eq!(res.status(), 400);
    Ok(())
}

// ---------- /programs/{id}/events ----------

#[tokio::test]
async fn program_events_filters() -> httpc_test::Result<()> {
    reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://events.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    let sig_a = signature(30);
    let sig_b = signature(31);
    let disc_a = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88];
    seed_event(&db, program_id, rpc_id, &sig_a, 0, 500, &[&disc_a[..], &[0xAB]].concat(), Some(1000)).await;
    seed_event(&db, program_id, rpc_id, &sig_b, 0, 600, &[0xAA; 9], Some(1001)).await;

    // all events, newest first
    let res = server()
        .unauthed_client()
        .do_get(&format!("/programs/{}/events", program_id))
        .await?;
    assert_eq!(res.status(), 200);
    let body = res.json_body_as::<Value>()?;
    assert_eq!(body["total"], 2);
    assert_eq!(body["data"][0]["slot"], 600);

    // since_slot keeps only the newer event
    let res = server()
        .unauthed_client()
        .do_get(&format!("/programs/{}/events?since_slot=600", program_id))
        .await?;
    assert_eq!(res.json_body_as::<Value>()?["total"], 1);

    // by transaction signature
    let res = server()
        .unauthed_client()
        .do_get(&format!("/programs/{}/events?tx_signature={}", program_id, sig_a))
        .await?;
    let body = res.json_body_as::<Value>()?;
    assert_eq!(body["total"], 1);
    assert_eq!(body["data"][0]["tx_signature"], sig_a.to_string());

    // by discriminator (leading 8 bytes as hex)
    let res = server()
        .unauthed_client()
        .do_get(&format!("/programs/{}/events?discriminator={}", program_id, hex::encode(disc_a)))
        .await?;
    assert_eq!(res.json_body_as::<Value>()?["total"], 1);
    Ok(())
}

#[tokio::test]
async fn program_events_invalid_discriminator_is_bad_request() -> httpc_test::Result<()> {
    reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://events-bad.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    let res = server()
        .unauthed_client()
        .do_get(&format!("/programs/{}/events?discriminator=zz", program_id))
        .await?;
    assert_eq!(res.status(), 400);
    Ok(())
}

// ---------- /programs/{id}/transactions & /transactions ----------

#[tokio::test]
async fn program_transactions_success_and_range_filters() -> httpc_test::Result<()> {
    reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://ptx.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    seed_transaction(&db, Some(program_id), rpc_id, &signature(40), 500, true, Some(1000)).await;
    seed_transaction(&db, Some(program_id), rpc_id, &signature(41), 600, false, Some(1001)).await;

    let res = server()
        .unauthed_client()
        .do_get(&format!("/programs/{}/transactions", program_id))
        .await?;
    assert_eq!(res.status(), 200);
    assert_eq!(res.json_body_as::<Value>()?["total"], 2);

    // only failed
    let res = server()
        .unauthed_client()
        .do_get(&format!("/programs/{}/transactions?success=false", program_id))
        .await?;
    let body = res.json_body_as::<Value>()?;
    assert_eq!(body["total"], 1);
    assert_eq!(body["data"][0]["signature"], signature(41).to_string());

    // only from slot 600 up
    let res = server()
        .unauthed_client()
        .do_get(&format!("/programs/{}/transactions?from_slot=600", program_id))
        .await?;
    assert_eq!(res.json_body_as::<Value>()?["total"], 1);
    Ok(())
}

#[tokio::test]
async fn get_transaction_by_signature_and_list() -> httpc_test::Result<()> {
    reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://tx.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    seed_transaction(&db, Some(program_id), rpc_id, &signature(40), 500, true, Some(1000)).await;
    seed_transaction(&db, None, rpc_id, &signature(42), 700, true, Some(1002)).await;

    // single by signature
    let res = server()
        .unauthed_client()
        .do_get(&format!("/transactions/{}", signature(40)))
        .await?;
    assert_eq!(res.status(), 200);
    let body = res.json_body_as::<Value>()?;
    assert_eq!(body["signature"], signature(40).to_string());
    assert_eq!(body["success"], true);
    assert_eq!(body["block_time"], 1000);

    // global list with since_slot
    let res = server()
        .unauthed_client()
        .do_get("/transactions?since_slot=650")
        .await?;
    let body = res.json_body_as::<Value>()?;
    assert_eq!(body["total"], 1);
    assert_eq!(body["data"][0]["signature"], signature(42).to_string());
    Ok(())
}

#[tokio::test]
async fn get_transaction_unknown_is_not_found() -> httpc_test::Result<()> {
    reset_db().await;
    let res = server()
        .unauthed_client()
        .do_get(&format!("/transactions/{}", signature(99)))
        .await?;
    assert_eq!(res.status(), 404);
    Ok(())
}

#[tokio::test]
async fn get_transaction_invalid_signature_is_bad_request() -> httpc_test::Result<()> {
    reset_db().await;
    let res = server()
        .unauthed_client()
        .do_get("/transactions/not-a-signature")
        .await?;
    assert_eq!(res.status(), 400);
    Ok(())
}

// ---------- /programs/{id}/changes ----------

#[tokio::test]
async fn program_changes_optionally_includes_events() -> httpc_test::Result<()> {
    reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://changes.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    seed_account(&db, program_id, rpc_id, &pubkey(10), &pubkey(20), &[1], 10, 500, 0, Some(1000)).await;
    seed_account(&db, program_id, rpc_id, &pubkey(11), &pubkey(21), &[2], 20, 600, 0, Some(1001)).await;
    seed_event(&db, program_id, rpc_id, &signature(30), 0, 550, &[7; 9], Some(1000)).await;

    // without events: the events fields are omitted
    let res = server()
        .unauthed_client()
        .do_get(&format!("/programs/{}/changes", program_id))
        .await?;
    assert_eq!(res.status(), 200);
    let body = res.json_body_as::<Value>()?;
    assert_eq!(body["account_states_total"], 2);
    assert!(body.get("events").is_none() || body["events"].is_null());

    // with events: present and ascending
    let res = server()
        .unauthed_client()
        .do_get(&format!("/programs/{}/changes?include_events=true", program_id))
        .await?;
    let body = res.json_body_as::<Value>()?;
    assert_eq!(body["account_states_total"], 2);
    assert_eq!(body["events_total"], 1);
    assert_eq!(body["account_states"][0]["slot"], 500);
    Ok(())
}
