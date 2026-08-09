use futures::StreamExt;
use sea_orm::DatabaseConnection;
use solana_sdk::pubkey::Pubkey;
use tokio::sync::broadcast;

use crate::core::error::SvmIndexerError;
use crate::core::sse::SseEvent;
use crate::db::entities::programs;
use crate::indexer::persist::upsert_account_snapshot;
use crate::indexer::rpc::SvmRpc;

/// Runs on every boot to fill the downtime gap by snapshoting every account a program owns.
pub async fn catch_up_state<R: SvmRpc + ?Sized>(
    rpc: &R,
    db: &DatabaseConnection,
    sse_tx: &broadcast::Sender<SseEvent>,
    program: &programs::Model,
) -> Result<(), SvmIndexerError> {
    let address = Pubkey::try_from(program.address.as_slice()).map_err(|_| {
        SvmIndexerError::InvalidPublicKey("Stored program address is not a valid pubkey!".to_owned())
    })?;
    let snapshots = rpc.get_program_accounts(&address, None).await?;

    // every snapshot shares the fetch slot, so resolve its block_time once
    let block_time = match snapshots.first() {
        Some(first) => rpc.get_block_time(first.slot).await?,
        None => return Ok(()),
    };

    for snapshot in &snapshots {
        upsert_account_snapshot(db, sse_tx, program.id, program.rpc_id, snapshot, None, block_time)
            .await?;
    }
    Ok(())
}

pub async fn stream_state<R: SvmRpc + ?Sized>(
    rpc: &R,
    db: &DatabaseConnection,
    sse_tx: &broadcast::Sender<SseEvent>,
    program: &programs::Model,
) -> Result<(), SvmIndexerError> {
    let address = Pubkey::try_from(program.address.as_slice()).map_err(|_| {
        SvmIndexerError::InvalidPublicKey("Stored program address is not a valid pubkey!".to_owned())
    })?;
    let mut stream = rpc.subscribe_program(&address, None).await?;

    while let Some(snapshot) = stream.next().await {
        let snapshot = snapshot?;
        upsert_account_snapshot(db, sse_tx, program.id, program.rpc_id, &snapshot, None, None)
            .await?;
    }
    Ok(())
}
