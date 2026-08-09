use sea_orm::DatabaseConnection;
use solana_sdk::pubkey::Pubkey;

use crate::core::error::SvmIndexerError;
use crate::db::entities::programs;
use crate::indexer::persist::upsert_account_snapshot;
use crate::indexer::rpc::SvmRpc;

/// Runs on every boot to fill the downtime gap by snapshoting every account a program owns.
pub async fn catch_up_state<R: SvmRpc + ?Sized>(
    rpc: &R,
    db: &DatabaseConnection,
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
        upsert_account_snapshot(db, program.id, program.rpc_id, snapshot, None, block_time).await?;
    }
    Ok(())
}
