use solana_sdk::{pubkey::Pubkey, signature::Signature};
use tokio::sync::broadcast;

use svm_indexer::core::commitment::TransactionCommitment;
use svm_indexer::core::sse::SseEvent;
use svm_indexer::indexer::persist::upsert_account_snapshot;
use svm_indexer::indexer::rpc::{AccountSnapshot, RawTransaction};
use svm_indexer::indexer::transactions::index_transaction;

use crate::common::{pubkey, reset_db, seed_program, seed_rpc, signature, test_db};

fn snapshot(address: &Pubkey, owner: &Pubkey, data: &[u8], slot: u64) -> AccountSnapshot {
    AccountSnapshot {
        address: *address,
        owner: *owner,
        lamports: 100,
        data: data.to_vec(),
        rent_epoch: 0,
        executable: false,
        slot,
        write_version: Some(0),
    }
}

fn raw_tx(sig: &Signature, slot: u64) -> RawTransaction {
    RawTransaction {
        signature: *sig,
        slot,
        block_time: Some(1000),
        success: true,
        err: None,
        fee: Some(5000),
        fee_payer: None,
        logs: vec![],
        accounts: vec![],
        raw: serde_json::json!({ "t": true }),
    }
}

#[tokio::test]
async fn rejected_older_write_is_not_broadcast() {
    let _guard = reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://reject-sse.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;
    let (sse_tx, mut rx) = broadcast::channel::<SseEvent>(16);
    let (addr, owner) = (pubkey(10), pubkey(20));

    // newer write advances the row → broadcasts
    upsert_account_snapshot(
        &db,
        &sse_tx,
        program_id,
        rpc_id,
        &snapshot(&addr, &owner, &[2], 600),
        None,
        Some(1001),
    )
    .await
    .unwrap();
    assert!(matches!(
        rx.try_recv(),
        Ok(SseEvent::AccountChanged { slot: 600, .. })
    ));

    // older write with different data → forward-only guard rejects it → no broadcast (#4)
    upsert_account_snapshot(
        &db,
        &sse_tx,
        program_id,
        rpc_id,
        &snapshot(&addr, &owner, &[3], 500),
        None,
        Some(1000),
    )
    .await
    .unwrap();
    assert!(
        rx.try_recv().is_err(),
        "a rejected older write must not broadcast"
    );
}

#[tokio::test]
async fn reingesting_a_transaction_does_not_rebroadcast() {
    let _guard = reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://reingest-sse.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;
    let (sse_tx, mut rx) = broadcast::channel::<SseEvent>(16);
    let addr = pubkey(2);
    let tx = raw_tx(&signature(7), 700);

    index_transaction(
        &db,
        &sse_tx,
        program_id,
        rpc_id,
        &addr,
        &tx,
        TransactionCommitment::Confirmed,
    )
    .await
    .unwrap();
    assert!(matches!(
        rx.try_recv(),
        Ok(SseEvent::TransactionIndexed { slot: 700, .. })
    ));
    while rx.try_recv().is_ok() {} // drain (none expected: empty logs → no events)

    // duplicate ingest → ON CONFLICT DO NOTHING → no rebroadcast (#13)
    index_transaction(
        &db,
        &sse_tx,
        program_id,
        rpc_id,
        &addr,
        &tx,
        TransactionCommitment::Confirmed,
    )
    .await
    .unwrap();
    assert!(
        rx.try_recv().is_err(),
        "a duplicate ingest must not rebroadcast"
    );
}
