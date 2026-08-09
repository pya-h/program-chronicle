use futures::StreamExt;
use sea_orm::DatabaseConnection;
use solana_sdk::pubkey::Pubkey;
use tokio::sync::broadcast;

use crate::core::error::SvmIndexerError;
use crate::core::sse::SseEvent;
use crate::db::entities::programs;
use crate::indexer::filter::parse_account_filters;
use crate::indexer::persist::upsert_account_snapshot;
use crate::indexer::rpc::SvmRpc;

/// Runs on every boot to fill the downtime gap by snapshoting the accounts a program owns (or the
/// filtered subset, when `account_filters` narrows it).
pub async fn catch_up_state<R: SvmRpc + ?Sized>(
    rpc: &R,
    db: &DatabaseConnection,
    sse_tx: &broadcast::Sender<SseEvent>,
    program: &programs::Model,
) -> Result<(), SvmIndexerError> {
    let address = Pubkey::try_from(program.address.as_slice()).map_err(|_| {
        SvmIndexerError::InvalidPublicKey("Stored program address is not a valid pubkey!".to_owned())
    })?;
    let filter = parse_account_filters(program.account_filters.as_ref());
    let snapshots = match filter.as_ref().and_then(|f| f.address_allowlist()) {
        // an address-only filter fetches just those keys instead of scanning the whole program
        Some(addresses) => rpc.get_multiple_accounts(&addresses).await?,
        None => rpc.get_program_accounts(&address, filter.as_ref()).await?,
    };

    // every snapshot shares the fetch slot, so resolve its block_time once
    let block_time = match snapshots.first() {
        Some(first) => rpc.get_block_time(first.slot).await?,
        None => return Ok(()),
    };

    for snapshot in &snapshots {
        // the RPC already narrowed data_size/memcmp; this also applies a combined addresses clause
        if filter.as_ref().is_some_and(|f| !f.matches(snapshot)) {
            continue;
        }
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
    let filter = parse_account_filters(program.account_filters.as_ref());
    let mut stream = match filter.as_ref().and_then(|f| f.address_allowlist()) {
        Some(addresses) => rpc.subscribe_accounts(&addresses).await?,
        None => rpc.subscribe_program(&address, filter.as_ref()).await?,
    };

    while let Some(snapshot) = stream.next().await {
        let snapshot = snapshot?;
        // predicate fallback: drop anything the server-side filter let through
        if filter.as_ref().is_some_and(|f| !f.matches(&snapshot)) {
            continue;
        }
        upsert_account_snapshot(db, sse_tx, program.id, program.rpc_id, &snapshot, None, None)
            .await?;
    }
    Ok(())
}
