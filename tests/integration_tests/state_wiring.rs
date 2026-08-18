use async_trait::async_trait;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use solana_sdk::{pubkey::Pubkey, signature::Signature};
use tokio::sync::broadcast;

use svm_indexer::core::commitment::TransactionCommitment;
use svm_indexer::core::sse::SseEvent;
use svm_indexer::db::entities::{account_states, program_accounts, programs};
use svm_indexer::indexer::rpc::{
    AccountFilter, AccountSnapshot, LogsNotification, RawTransaction, RpcResult, SignatureInfo,
    SubReady, SubStream, SvmRpc,
};
use svm_indexer::indexer::state::catch_up_state;

use crate::common::{pubkey, reset_db, seed_program, seed_rpc, test_db};

/// A stand-in RPC whose program scan returns exactly one snapshot, enough to drive `catch_up_state`.
struct SingleSnapshotRpc {
    snapshot: AccountSnapshot,
    block_time: Option<i64>,
}

#[async_trait]
impl SvmRpc for SingleSnapshotRpc {
    async fn get_slot(&self) -> RpcResult<u64> {
        Ok(self.snapshot.slot)
    }

    async fn get_block_time(&self, _slot: u64) -> RpcResult<Option<i64>> {
        Ok(self.block_time)
    }

    async fn get_program_accounts(
        &self,
        _program: &Pubkey,
        _filter: Option<&AccountFilter>,
    ) -> RpcResult<Vec<AccountSnapshot>> {
        Ok(vec![self.snapshot.clone()])
    }

    async fn get_multiple_accounts(
        &self,
        _addresses: &[Pubkey],
    ) -> RpcResult<Vec<AccountSnapshot>> {
        Ok(vec![self.snapshot.clone()])
    }

    async fn get_signatures_for_address(
        &self,
        _address: &Pubkey,
        _before: Option<Signature>,
        _limit: usize,
    ) -> RpcResult<Vec<SignatureInfo>> {
        Ok(vec![])
    }

    async fn get_transaction(&self, _signature: &Signature) -> RpcResult<RawTransaction> {
        unimplemented!("catch_up_state never fetches transactions")
    }

    async fn get_signature_statuses(
        &self,
        _signatures: &[Signature],
    ) -> RpcResult<Vec<Option<TransactionCommitment>>> {
        Ok(vec![])
    }

    async fn get_signature_statuses_with_history(
        &self,
        _signatures: &[Signature],
    ) -> RpcResult<Vec<Option<TransactionCommitment>>> {
        Ok(vec![])
    }

    async fn subscribe_program(
        &self,
        _program: &Pubkey,
        _filter: Option<&AccountFilter>,
        _ready: SubReady,
    ) -> RpcResult<SubStream<AccountSnapshot>> {
        unimplemented!("catch_up_state never subscribes")
    }

    async fn subscribe_accounts(
        &self,
        _addresses: &[Pubkey],
        _ready: SubReady,
    ) -> RpcResult<SubStream<AccountSnapshot>> {
        unimplemented!("catch_up_state never subscribes")
    }

    async fn subscribe_logs(
        &self,
        _program: &Pubkey,
        _ready: SubReady,
    ) -> RpcResult<SubStream<LogsNotification>> {
        unimplemented!("catch_up_state never subscribes")
    }
}

fn snapshot(address: &Pubkey, owner: &Pubkey, data: &[u8], slot: u64) -> AccountSnapshot {
    AccountSnapshot {
        address: *address,
        owner: *owner,
        lamports: 100,
        data: data.to_vec(),
        rent_epoch: 0,
        executable: false,
        slot,
        write_version: None,
    }
}

async fn history_len(db: &sea_orm::DatabaseConnection, program_id: i32, address: &Pubkey) -> usize {
    let account = program_accounts::Entity::find()
        .filter(program_accounts::Column::ProgramId.eq(program_id))
        .filter(program_accounts::Column::Address.eq(address.to_bytes().to_vec()))
        .one(db)
        .await
        .unwrap()
        .expect("cache row should exist after catch-up");
    account_states::Entity::find()
        .filter(account_states::Column::ProgramAccountId.eq(account.id))
        .all(db)
        .await
        .unwrap()
        .len()
}

#[tokio::test]
async fn catch_up_records_history_when_tracking_enabled() {
    let _guard = reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://catchup-track-on.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;
    let program = programs::Entity::find_by_id(program_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(
        program.track_account_states,
        "seeded programs default to tracking"
    );

    let address = pubkey(10);
    let rpc = SingleSnapshotRpc {
        snapshot: snapshot(&address, &pubkey(20), &[1], 500),
        block_time: Some(1000),
    };
    let (sse_tx, _) = broadcast::channel::<SseEvent>(16);
    catch_up_state(&rpc, &db, &sse_tx, &program).await.unwrap();

    assert_eq!(
        history_len(&db, program_id, &address).await,
        1,
        "a tracking program records the catch-up snapshot as history"
    );
}

#[tokio::test]
async fn catch_up_skips_history_when_tracking_disabled() {
    let _guard = reset_db().await;
    let db = test_db().await;
    let rpc_id = seed_rpc(&db, "https://catchup-track-off.example.com").await;
    let program_id = seed_program(&db, rpc_id, &pubkey(1)).await;
    let seeded = programs::Entity::find_by_id(program_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    // flip the flag off on the in-memory model catch_up_state reads from
    let program = programs::Model {
        track_account_states: false,
        ..seeded
    };

    let address = pubkey(10);
    let rpc = SingleSnapshotRpc {
        snapshot: snapshot(&address, &pubkey(20), &[1], 500),
        block_time: Some(1000),
    };
    let (sse_tx, _) = broadcast::channel::<SseEvent>(16);
    catch_up_state(&rpc, &db, &sse_tx, &program).await.unwrap();

    // the latest state is still cached, but no history row is written
    assert_eq!(
        history_len(&db, program_id, &address).await,
        0,
        "an untracked program caches latest state without appending history"
    );
}
