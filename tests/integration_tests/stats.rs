use serde_json::Value;

use crate::common::{
    pubkey, reset_db, seed_account, seed_event, seed_program, seed_rpc, seed_transaction, server,
    signature, test_db,
};

#[tokio::test]
async fn stats_reports_counts_and_per_program_progress() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let db = test_db().await;

    let rpc_id = seed_rpc(&db, "https://stats.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;
    seed_account(
        &db,
        program_id,
        rpc_id,
        &pubkey(10),
        &pubkey(20),
        &[1, 2, 3],
        5,
        100,
        0,
        Some(1),
    )
    .await;
    seed_transaction(
        &db,
        Some(program_id),
        rpc_id,
        &signature(1),
        100,
        true,
        Some(1),
    )
    .await;
    seed_event(
        &db,
        program_id,
        rpc_id,
        &signature(1),
        0,
        100,
        &[9; 8],
        Some(1),
    )
    .await;

    let res = server().authed_client().do_get("/admin/stats").await?;
    assert_eq!(res.status(), 200);

    let body = res.json_body_as::<Value>()?;
    // programs/rpcs use exact COUNT, so they must match what we seeded
    assert_eq!(body["programs"], 1);
    assert_eq!(body["rpcs"], 1);

    // per-program progress is read straight off the programs row
    let per_program = body["per_program"].as_array().expect("per_program array");
    assert_eq!(per_program.len(), 1);
    assert_eq!(per_program[0]["id"], program_id);
    assert_eq!(per_program[0]["address"], pubkey(1).to_string());
    assert_eq!(per_program[0]["start_slot"], 0);
    assert!(per_program[0]["phase"].is_null());

    // large-table totals come from pg_stat_user_tables.n_live_tup, not COUNT — the collector
    // reports it a moment after commit, so assert the shape, never exact values
    let est = &body["estimates"];
    for key in ["slots", "transactions", "events", "accounts"] {
        assert!(est[key].is_i64(), "estimate `{key}` should be an integer");
        assert!(
            est[key].as_i64().unwrap() >= 0,
            "estimate `{key}` should be non-negative"
        );
    }
    Ok(())
}

#[tokio::test]
async fn stats_without_auth_is_unauthorized() -> httpc_test::Result<()> {
    let _guard = reset_db().await;
    let res = server().unauthed_client().do_get("/admin/stats").await?;
    assert_eq!(res.status(), 401);
    Ok(())
}
