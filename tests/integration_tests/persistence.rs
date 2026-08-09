use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use svm_indexer::db::entities::{account_states, program_accounts, slots};
use svm_indexer::indexer::persist::ensure_slot;

use crate::common::{pubkey, reset_db, seed_account, seed_program, seed_rpc, test_db};

async fn cached_account(
    db: &sea_orm::DatabaseConnection,
    program_id: i32,
    address: &solana_sdk::pubkey::Pubkey,
) -> program_accounts::Model {
    program_accounts::Entity::find()
        .filter(program_accounts::Column::ProgramId.eq(program_id))
        .filter(program_accounts::Column::Address.eq(address.to_bytes().to_vec()))
        .one(db)
        .await
        .unwrap()
        .expect("cached account should exist")
}

async fn history_rows(
    db: &sea_orm::DatabaseConnection,
    program_account_id: i64,
) -> Vec<account_states::Model> {
    account_states::Entity::find()
        .filter(account_states::Column::ProgramAccountId.eq(program_account_id))
        .all(db)
        .await
        .unwrap()
}

#[tokio::test]
async fn ensure_slot_fills_block_time_once_and_is_idempotent() {
    reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://ensure-slot.example.com").await;

    // first sighting has no block_time yet
    let id1 = ensure_slot(&db, rpc_id, 500, None).await.unwrap();
    let id2 = ensure_slot(&db, rpc_id, 500, Some(1000)).await.unwrap();
    assert_eq!(id1, id2, "same (rpc, slot) must return the same row id");

    let row = slots::Entity::find_by_id(id1).one(&db).await.unwrap().unwrap();
    assert_eq!(row.block_time, Some(1000), "block_time should fill once known");

    // a later, different block_time must not overwrite the first known value
    let id3 = ensure_slot(&db, rpc_id, 500, Some(2000)).await.unwrap();
    assert_eq!(id3, id1);
    let row = slots::Entity::find_by_id(id1).one(&db).await.unwrap().unwrap();
    assert_eq!(row.block_time, Some(1000), "block_time is write-once");
}

#[tokio::test]
async fn double_apply_identical_snapshot_appends_no_extra_history() {
    reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://double-apply.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    let address = pubkey(10);
    let owner = pubkey(20);
    // apply the very same snapshot twice, as a reboot's catch-up would
    seed_account(&db, program_id, rpc_id, &address, &owner, &[1, 2, 3], 100, 500, 0, Some(1000)).await;
    seed_account(&db, program_id, rpc_id, &address, &owner, &[1, 2, 3], 100, 500, 0, Some(1000)).await;

    let account = cached_account(&db, program_id, &address).await;
    let history = history_rows(&db, account.id).await;
    assert_eq!(history.len(), 1, "an unchanged re-apply must not append history");
}

#[tokio::test]
async fn newer_slot_advances_cache_and_appends_history() {
    reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://newer-slot.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    let address = pubkey(10);
    let owner = pubkey(20);
    seed_account(&db, program_id, rpc_id, &address, &owner, &[1], 100, 500, 0, Some(1000)).await;
    seed_account(&db, program_id, rpc_id, &address, &owner, &[2], 100, 600, 0, Some(1001)).await;

    let account = cached_account(&db, program_id, &address).await;
    assert_eq!(account.latest_slot, 600, "cache must advance to the newer slot");
    assert_eq!(account.state, vec![2]);
    assert_eq!(history_rows(&db, account.id).await.len(), 2);
}

#[tokio::test]
async fn older_slot_does_not_regress_cache_but_records_history() {
    reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://older-slot.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    let address = pubkey(10);
    let owner = pubkey(20);
    seed_account(&db, program_id, rpc_id, &address, &owner, &[2], 100, 600, 0, Some(1001)).await;
    // an out-of-order older slot arrives after the newer one
    seed_account(&db, program_id, rpc_id, &address, &owner, &[1], 100, 500, 0, Some(1000)).await;

    let account = cached_account(&db, program_id, &address).await;
    assert_eq!(account.latest_slot, 600, "an older slot must not regress the cache");
    assert_eq!(account.state, vec![2]);
    // but the older state is still recorded for history reads
    assert_eq!(history_rows(&db, account.id).await.len(), 2);
}

#[tokio::test]
async fn higher_write_version_advances_cache_within_same_slot() {
    reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://write-version.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;

    let address = pubkey(10);
    let owner = pubkey(20);
    seed_account(&db, program_id, rpc_id, &address, &owner, &[1], 100, 700, 0, Some(1000)).await;
    seed_account(&db, program_id, rpc_id, &address, &owner, &[2], 100, 700, 1, Some(1000)).await;

    let account = cached_account(&db, program_id, &address).await;
    assert_eq!(account.latest_slot, 700);
    assert_eq!(account.latest_write_version, 1, "higher write_version wins within a slot");
    assert_eq!(account.state, vec![2]);
    assert_eq!(history_rows(&db, account.id).await.len(), 2);
}
