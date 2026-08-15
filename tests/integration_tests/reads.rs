use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::Value;

use crate::common::{
    pubkey, reset_db, seed_account, seed_event, seed_program, seed_rpc, seed_transaction,
    seed_transaction_account, server, set_transaction_commitment, signature, test_db,
};

fn b64(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

// ---------- /accounts ----------

#[tokio::test]
async fn get_account_returns_latest_state() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://get-account.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    let address = pubkey(10);
    let owner = pubkey(20);
    seed_account(
        &db,
        program_id,
        rpc_id,
        &address,
        &owner,
        &[9, 8, 7],
        42,
        500,
        0,
        Some(1000),
    )
    .await;
    // a newer state, which should be the one served
    seed_account(
        &db,
        program_id,
        rpc_id,
        &address,
        &owner,
        &[1, 1, 1],
        43,
        600,
        0,
        Some(1001),
    )
    .await;

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
    let _guard = reset_db().await;
    let res = server()
        .unauthed_client()
        .do_get(&format!("/accounts/{}", pubkey(99)))
        .await?;
    assert_eq!(res.status(), 404);
    Ok(())
}

#[tokio::test]
async fn get_account_invalid_address_is_bad_request() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let res = server()
        .unauthed_client()
        .do_get("/accounts/not-a-real-address")
        .await?;
    assert_eq!(res.status(), 400);
    Ok(())
}

#[tokio::test]
async fn get_multiple_accounts_is_positional_with_nulls() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://multi-account.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    let a = pubkey(10);
    let c = pubkey(12);
    seed_account(
        &db,
        program_id,
        rpc_id,
        &a,
        &pubkey(20),
        &[1],
        10,
        500,
        0,
        Some(1000),
    )
    .await;
    seed_account(
        &db,
        program_id,
        rpc_id,
        &c,
        &pubkey(22),
        &[3],
        30,
        500,
        0,
        Some(1000),
    )
    .await;

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
    let _guard = reset_db().await;
    let res = server()
        .unauthed_client()
        .do_get("/accounts?addresses=%20")
        .await?;
    assert_eq!(res.status(), 400);
    Ok(())
}

#[tokio::test]
async fn get_account_history_with_range_and_pagination() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://history.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    let address = pubkey(10);
    let owner = pubkey(20);
    for (i, slot) in [500_u64, 600, 700, 800].into_iter().enumerate() {
        seed_account(
            &db,
            program_id,
            rpc_id,
            &address,
            &owner,
            &[i as u8],
            10,
            slot,
            0,
            Some(1000 + slot as i64),
        )
        .await;
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
        .do_get(&format!(
            "/accounts/{}/history?from_slot=600&to_slot=700",
            address
        ))
        .await?;
    let body = res.json_body_as::<Value>()?;
    assert_eq!(body["total"], 2);
    let slots: Vec<i64> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["slot"].as_i64().unwrap())
        .collect();
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
    let _guard = reset_db().await;
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
    let _guard = reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://gpa.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    let a = pubkey(10);
    let b = pubkey(11);
    seed_account(
        &db,
        program_id,
        rpc_id,
        &a,
        &pubkey(20),
        &[0xAA, 0xBB, 0, 0, 0, 0, 0, 0],
        10,
        500,
        0,
        Some(1000),
    )
    .await;
    seed_account(
        &db,
        program_id,
        rpc_id,
        &b,
        &pubkey(21),
        &[1, 2, 3, 4],
        20,
        501,
        0,
        Some(1001),
    )
    .await;

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
        .do_get(&format!(
            "/programs/{}/accounts?memcmp=0:{}",
            program_id, prefix
        ))
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
        .do_get(&format!(
            "/programs/{}/accounts?page=1&per_page=1",
            program_id
        ))
        .await?;
    let body = res.json_body_as::<Value>()?;
    assert_eq!(body["total"], 2);
    assert_eq!(body["pages"], 2);
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
    Ok(())
}

#[tokio::test]
async fn program_accounts_malformed_memcmp_is_bad_request() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://gpa-bad.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    let res = server()
        .unauthed_client()
        .do_get(&format!(
            "/programs/{}/accounts?memcmp=not-an-offset",
            program_id
        ))
        .await?;
    assert_eq!(res.status(), 400);
    Ok(())
}

// ---------- /programs/{id}/events ----------

#[tokio::test]
async fn program_events_filters() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://events.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    let sig_a = signature(30);
    let sig_b = signature(31);
    let disc_a = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88];
    seed_event(
        &db,
        program_id,
        rpc_id,
        &sig_a,
        0,
        500,
        &[&disc_a[..], &[0xAB]].concat(),
        Some(1000),
    )
    .await;
    seed_event(
        &db,
        program_id,
        rpc_id,
        &sig_b,
        0,
        600,
        &[0xAA; 9],
        Some(1001),
    )
    .await;

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
        .do_get(&format!(
            "/programs/{}/events?tx_signature={}",
            program_id, sig_a
        ))
        .await?;
    let body = res.json_body_as::<Value>()?;
    assert_eq!(body["total"], 1);
    assert_eq!(body["data"][0]["tx_signature"], sig_a.to_string());

    // by discriminator (leading 8 bytes as hex)
    let res = server()
        .unauthed_client()
        .do_get(&format!(
            "/programs/{}/events?discriminator={}",
            program_id,
            hex::encode(disc_a)
        ))
        .await?;
    assert_eq!(res.json_body_as::<Value>()?["total"], 1);
    Ok(())
}

#[tokio::test]
async fn program_events_invalid_discriminator_is_bad_request() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
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
    let _guard = reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://ptx.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    seed_transaction(
        &db,
        Some(program_id),
        rpc_id,
        &signature(40),
        500,
        true,
        Some(1000),
    )
    .await;
    seed_transaction(
        &db,
        Some(program_id),
        rpc_id,
        &signature(41),
        600,
        false,
        Some(1001),
    )
    .await;

    let res = server()
        .unauthed_client()
        .do_get(&format!("/programs/{}/transactions", program_id))
        .await?;
    assert_eq!(res.status(), 200);
    assert_eq!(res.json_body_as::<Value>()?["total"], 2);

    // only failed
    let res = server()
        .unauthed_client()
        .do_get(&format!(
            "/programs/{}/transactions?success=false",
            program_id
        ))
        .await?;
    let body = res.json_body_as::<Value>()?;
    assert_eq!(body["total"], 1);
    assert_eq!(body["data"][0]["signature"], signature(41).to_string());

    // only from slot 600 up
    let res = server()
        .unauthed_client()
        .do_get(&format!(
            "/programs/{}/transactions?from_slot=600",
            program_id
        ))
        .await?;
    assert_eq!(res.json_body_as::<Value>()?["total"], 1);
    Ok(())
}

#[tokio::test]
async fn get_transaction_by_signature_and_list() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://tx.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    seed_transaction(
        &db,
        Some(program_id),
        rpc_id,
        &signature(40),
        500,
        true,
        Some(1000),
    )
    .await;
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
    let _guard = reset_db().await;
    let res = server()
        .unauthed_client()
        .do_get(&format!("/transactions/{}", signature(99)))
        .await?;
    assert_eq!(res.status(), 404);
    Ok(())
}

#[tokio::test]
async fn transactions_surface_commitment_and_hide_dropped_by_default() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://commitment.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    // ingest defaults to `confirmed`; mark one row reorged
    seed_transaction(
        &db,
        Some(program_id),
        rpc_id,
        &signature(40),
        500,
        true,
        Some(1000),
    )
    .await;
    seed_transaction(
        &db,
        Some(program_id),
        rpc_id,
        &signature(41),
        600,
        true,
        Some(1001),
    )
    .await;
    set_transaction_commitment(&db, &signature(41), "dropped").await;

    // default list hides the dropped row and surfaces the commitment field
    let body = server()
        .unauthed_client()
        .do_get(&format!("/programs/{}/transactions", program_id))
        .await?
        .json_body_as::<Value>()?;
    assert_eq!(body["total"], 1);
    assert_eq!(body["data"][0]["signature"], signature(40).to_string());
    assert_eq!(body["data"][0]["commitment"], "confirmed");

    // an explicit filter selects exactly that level
    let body = server()
        .unauthed_client()
        .do_get(&format!(
            "/programs/{}/transactions?commitment=dropped",
            program_id
        ))
        .await?
        .json_body_as::<Value>()?;
    assert_eq!(body["total"], 1);
    assert_eq!(body["data"][0]["signature"], signature(41).to_string());
    assert_eq!(body["data"][0]["commitment"], "dropped");

    // the global list hides it too
    let body = server()
        .unauthed_client()
        .do_get("/transactions")
        .await?
        .json_body_as::<Value>()?;
    assert_eq!(body["total"], 1);

    // a single fetch by signature still returns the dropped row, with its commitment
    let body = server()
        .unauthed_client()
        .do_get(&format!("/transactions/{}", signature(41)))
        .await?
        .json_body_as::<Value>()?;
    assert_eq!(body["commitment"], "dropped");
    Ok(())
}

// ---------- I5: multi-program correlation ----------

#[tokio::test]
async fn single_transaction_derives_all_participating_programs() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://corr-single.example.com").await;
    let program_one = seed_program(&db, rpc_id, &pubkey(1)).await;
    let program_two = seed_program(&db, rpc_id, &pubkey(2)).await;

    // each program owns one account; a shared tx references both, attributed to program_one first
    let account_one = pubkey(10);
    let account_two = pubkey(20);
    seed_account(
        &db,
        program_one,
        rpc_id,
        &account_one,
        &pubkey(1),
        &[1],
        1,
        500,
        1,
        Some(1000),
    )
    .await;
    seed_account(
        &db,
        program_two,
        rpc_id,
        &account_two,
        &pubkey(2),
        &[2],
        1,
        500,
        1,
        Some(1000),
    )
    .await;
    seed_transaction(
        &db,
        Some(program_one),
        rpc_id,
        &signature(40),
        500,
        true,
        Some(1000),
    )
    .await;
    seed_transaction_account(&db, &signature(40), &account_one, 500, true).await;
    seed_transaction_account(&db, &signature(40), &account_two, 500, false).await;

    let body = server()
        .unauthed_client()
        .do_get(&format!("/transactions/{}", signature(40)))
        .await?
        .json_body_as::<Value>()?;
    // scalar attribution stays the first ingester; the derived set is lossless
    assert_eq!(body["program_id"], program_one);
    let mut programs: Vec<i64> =
        serde_json::from_value(body["programs"].clone()).expect("programs array");
    programs.sort_unstable();
    assert_eq!(programs, vec![program_one as i64, program_two as i64]);
    Ok(())
}

#[tokio::test]
async fn correlated_program_transactions_are_opt_in_and_bounded() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://corr-list.example.com").await;
    let program_one = seed_program(&db, rpc_id, &pubkey(1)).await;
    let program_two = seed_program(&db, rpc_id, &pubkey(2)).await;

    let account_two = pubkey(20);
    seed_account(
        &db,
        program_two,
        rpc_id,
        &account_two,
        &pubkey(2),
        &[2],
        1,
        500,
        1,
        Some(1000),
    )
    .await;

    // sig40: program_two participates via an owned account; attributed to program_one
    seed_transaction(
        &db,
        Some(program_one),
        rpc_id,
        &signature(40),
        500,
        true,
        Some(1000),
    )
    .await;
    seed_transaction_account(&db, &signature(40), &account_two, 500, true).await;
    // sig41: program_two participates via an emitted event; also attributed to program_one
    seed_transaction(
        &db,
        Some(program_one),
        rpc_id,
        &signature(41),
        600,
        true,
        Some(1001),
    )
    .await;
    seed_event(
        &db,
        program_two,
        rpc_id,
        &signature(41),
        0,
        600,
        &[9],
        Some(1001),
    )
    .await;

    // default (lossy) list for program_two sees neither — it owns the scalar on neither row
    let body = server()
        .unauthed_client()
        .do_get(&format!("/programs/{}/transactions", program_two))
        .await?
        .json_body_as::<Value>()?;
    assert_eq!(body["total"], 0);

    // correlated list surfaces both, via the account and the event paths
    let body = server()
        .unauthed_client()
        .do_get(&format!(
            "/programs/{}/transactions?correlated=true&from_slot=0",
            program_two
        ))
        .await?
        .json_body_as::<Value>()?;
    assert_eq!(body["total"], 2);

    // correlated without a slot bound is rejected
    let res = server()
        .unauthed_client()
        .do_get(&format!(
            "/programs/{}/transactions?correlated=true",
            program_two
        ))
        .await?;
    assert_eq!(res.status(), 400);
    Ok(())
}

#[tokio::test]
async fn get_transaction_invalid_signature_is_bad_request() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let res = server()
        .unauthed_client()
        .do_get("/transactions/not-a-signature")
        .await?;
    assert_eq!(res.status(), 400);
    Ok(())
}

#[tokio::test]
async fn account_transactions_by_address_with_range() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://acct-tx.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    let address = pubkey(10);
    // three transactions touch the address; a fourth touches a different one
    seed_transaction(
        &db,
        Some(program_id),
        rpc_id,
        &signature(40),
        500,
        true,
        Some(1000),
    )
    .await;
    seed_transaction(
        &db,
        Some(program_id),
        rpc_id,
        &signature(41),
        600,
        true,
        Some(1001),
    )
    .await;
    seed_transaction(
        &db,
        Some(program_id),
        rpc_id,
        &signature(42),
        700,
        false,
        Some(1002),
    )
    .await;
    seed_transaction(
        &db,
        Some(program_id),
        rpc_id,
        &signature(43),
        800,
        true,
        Some(1003),
    )
    .await;
    seed_transaction_account(&db, &signature(40), &address, 500, true).await;
    seed_transaction_account(&db, &signature(41), &address, 600, false).await;
    seed_transaction_account(&db, &signature(42), &address, 700, true).await;
    seed_transaction_account(&db, &signature(43), &pubkey(11), 800, true).await;

    // all mentions of the address, newest slot first
    let res = server()
        .unauthed_client()
        .do_get(&format!("/accounts/{}/transactions", address))
        .await?;
    assert_eq!(res.status(), 200);
    let body = res.json_body_as::<Value>()?;
    assert_eq!(body["total"], 3);
    let sigs: Vec<String> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["signature"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        sigs,
        vec![
            signature(42).to_string(),
            signature(41).to_string(),
            signature(40).to_string(),
        ]
    );

    // bounded range keeps only the middle transaction
    let res = server()
        .unauthed_client()
        .do_get(&format!(
            "/accounts/{}/transactions?from_slot=600&to_slot=600",
            address
        ))
        .await?;
    let body = res.json_body_as::<Value>()?;
    assert_eq!(body["total"], 1);
    assert_eq!(body["data"][0]["signature"], signature(41).to_string());

    // pagination envelope
    let res = server()
        .unauthed_client()
        .do_get(&format!(
            "/accounts/{}/transactions?page=1&per_page=2",
            address
        ))
        .await?;
    let body = res.json_body_as::<Value>()?;
    assert_eq!(body["total"], 3);
    assert_eq!(body["pages"], 2);
    assert_eq!(body["data"].as_array().unwrap().len(), 2);
    Ok(())
}

#[tokio::test]
async fn account_transactions_invalid_address_is_bad_request() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let res = server()
        .unauthed_client()
        .do_get("/accounts/not-a-real-address/transactions")
        .await?;
    assert_eq!(res.status(), 400);
    Ok(())
}

// ---------- /programs/{id}/changes ----------

#[tokio::test]
async fn program_changes_optionally_includes_events() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://changes.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    seed_account(
        &db,
        program_id,
        rpc_id,
        &pubkey(10),
        &pubkey(20),
        &[1],
        10,
        500,
        0,
        Some(1000),
    )
    .await;
    seed_account(
        &db,
        program_id,
        rpc_id,
        &pubkey(11),
        &pubkey(21),
        &[2],
        20,
        600,
        0,
        Some(1001),
    )
    .await;
    seed_event(
        &db,
        program_id,
        rpc_id,
        &signature(30),
        0,
        550,
        &[7; 9],
        Some(1000),
    )
    .await;

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
        .do_get(&format!(
            "/programs/{}/changes?include_events=true",
            program_id
        ))
        .await?;
    let body = res.json_body_as::<Value>()?;
    assert_eq!(body["account_states_total"], 2);
    assert_eq!(body["events_total"], 1);
    assert_eq!(body["account_states"][0]["slot"], 500);
    Ok(())
}

// ---------- validation & pagination edges ----------

#[tokio::test]
async fn partial_pagination_still_paginates() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://partial-page.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    for seed in 40..43 {
        seed_transaction(
            &db,
            Some(program_id),
            rpc_id,
            &signature(seed),
            500 + seed as i64,
            true,
            Some(1000),
        )
        .await;
    }

    // a lone per_page must paginate (default page 1), not dump the whole table in one page
    let body = server()
        .unauthed_client()
        .do_get(&format!("/programs/{}/transactions?per_page=2", program_id))
        .await?
        .json_body_as::<Value>()?;
    assert_eq!(body["total"], 3);
    assert_eq!(body["pages"], 2, "3 rows / 2 per page => 2 pages");
    assert_eq!(body["data"].as_array().unwrap().len(), 2);

    // a lone page uses the default per_page; page 0 is treated as page 1
    let body = server()
        .unauthed_client()
        .do_get(&format!("/programs/{}/transactions?page=0", program_id))
        .await?
        .json_body_as::<Value>()?;
    assert_eq!(body["data"].as_array().unwrap().len(), 3);
    Ok(())
}

#[tokio::test]
async fn commitment_query_rejects_unknown_level() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://bad-commit.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    let res = server()
        .unauthed_client()
        .do_get("/transactions?commitment=finalizd")
        .await?;
    assert_eq!(
        res.status(),
        400,
        "a typo'd commitment is rejected, not silently empty"
    );

    let res = server()
        .unauthed_client()
        .do_get(&format!(
            "/programs/{}/transactions?commitment=bogus",
            program_id
        ))
        .await?;
    assert_eq!(res.status(), 400);
    Ok(())
}

#[tokio::test]
async fn program_accounts_negative_data_size_is_bad_request() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://neg-size.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    let res = server()
        .unauthed_client()
        .do_get(&format!("/programs/{}/accounts?dataSize=-1", program_id))
        .await?;
    assert_eq!(res.status(), 400);
    Ok(())
}

#[tokio::test]
async fn program_accounts_memcmp_invalid_base58_and_huge_offset() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://memcmp-edges.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    // valid offset, but the bytes aren't base58
    let res = server()
        .unauthed_client()
        .do_get(&format!("/programs/{}/accounts?memcmp=0:0OIl", program_id))
        .await?;
    assert_eq!(res.status(), 400);

    // an offset past the account-size cap is rejected cleanly instead of overflowing `offset + 1`
    let res = server()
        .unauthed_client()
        .do_get(&format!(
            "/programs/{}/accounts?memcmp=99999999999999999999:AA",
            program_id
        ))
        .await?;
    assert_eq!(res.status(), 400);
    Ok(())
}

#[tokio::test]
async fn program_events_invalid_tx_signature_is_bad_request() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://events-badsig.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    let res = server()
        .unauthed_client()
        .do_get(&format!(
            "/programs/{}/events?tx_signature=not-a-signature",
            program_id
        ))
        .await?;
    assert_eq!(res.status(), 400);
    Ok(())
}

#[tokio::test]
async fn correlated_requires_a_lower_slot_bound() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://corr-bound.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    // a lone to_slot leaves the multi-hop scan unbounded below, so it must be rejected
    let res = server()
        .unauthed_client()
        .do_get(&format!(
            "/programs/{}/transactions?correlated=true&to_slot=1000",
            program_id
        ))
        .await?;
    assert_eq!(res.status(), 400);
    Ok(())
}

#[tokio::test]
async fn transaction_programs_always_include_the_ingester() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://prog-superset.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    // a tx the program ingested, but which references no tracked account and emitted no parsed event
    seed_transaction(
        &db,
        Some(program_id),
        rpc_id,
        &signature(40),
        500,
        true,
        Some(1000),
    )
    .await;

    let body = server()
        .unauthed_client()
        .do_get(&format!("/transactions/{}", signature(40)))
        .await?
        .json_body_as::<Value>()?;
    // the derived set is still a superset of the scalar program_id
    let programs: Vec<i64> =
        serde_json::from_value(body["programs"].clone()).expect("programs array");
    assert_eq!(programs, vec![program_id as i64]);
    Ok(())
}

#[tokio::test]
async fn get_multiple_accounts_invalid_address_is_bad_request() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let _db = test_db().await;

    let res = server()
        .unauthed_client()
        .do_get("/accounts?addresses=not-a-valid-address")
        .await?;
    assert_eq!(res.status(), 400);
    Ok(())
}

#[tokio::test]
async fn account_read_is_deterministic_when_shared_by_programs() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://shared-addr.example.com").await;
    // program_one is seeded first, so it holds the lower id — the deterministic winner
    let program_one = seed_program(&db, rpc_id, &pubkey(1)).await;
    let program_two = seed_program(&db, rpc_id, &pubkey(2)).await;

    let address = pubkey(10);
    seed_account(
        &db,
        program_one,
        rpc_id,
        &address,
        &pubkey(20),
        &[1],
        100,
        500,
        0,
        Some(1000),
    )
    .await;
    seed_account(
        &db,
        program_two,
        rpc_id,
        &address,
        &pubkey(21),
        &[2],
        100,
        500,
        0,
        Some(1000),
    )
    .await;

    // both endpoints resolve the same, lowest-program_id row rather than an arbitrary one
    for _ in 0..3 {
        let body = server()
            .unauthed_client()
            .do_get(&format!("/accounts/{}", address))
            .await?
            .json_body_as::<Value>()?;
        assert_eq!(body["owner"], pubkey(20).to_string());
    }
    Ok(())
}
