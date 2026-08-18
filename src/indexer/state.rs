use futures::StreamExt;
use sea_orm::DatabaseConnection;
use solana_sdk::pubkey::Pubkey;
use tokio::sync::broadcast;

use crate::core::error::SvmIndexerError;
use crate::core::indexer_error::{IndexerErrorKind, record};
use crate::core::sse::SseEvent;
use crate::db::entities::programs;
use crate::indexer::filter::parse_account_filters;
use crate::indexer::persist::upsert_account_snapshot;
use crate::indexer::rpc::{AccountFilter, AccountSnapshot, MemcmpFilter, SubReady, SvmRpc};

/// `memcmp` is byte-granular, so a one-byte prefix always splits a program's accounts into exactly
/// this many disjoint buckets — the count is not tunable, only the offset is.
const SCAN_BUCKETS: u16 = 256;

/// Runs on every boot to fill the downtime gap by snapshoting the accounts a program owns (or the
/// filtered subset, when `account_filters` narrows it).
pub async fn catch_up_state<R: SvmRpc + ?Sized>(
    rpc: &R,
    db: &DatabaseConnection,
    sse_tx: &broadcast::Sender<SseEvent>,
    program: &programs::Model,
) -> Result<(), SvmIndexerError> {
    let address = Pubkey::try_from(program.address.as_slice()).map_err(|_| {
        SvmIndexerError::InvalidPublicKey(
            "Stored program address is not a valid pubkey!".to_owned(),
        )
    })?;
    let filter = parse_account_filters(program.account_filters.as_ref());
    let snapshots = match filter.as_ref().and_then(|f| f.address_allowlist()) {
        // an address-only filter fetches just those keys instead of scanning the whole program
        Some(addresses) => rpc.get_multiple_accounts(&addresses).await?,
        None => match scan_program(rpc, db, program, &address, filter.as_ref()).await {
            Some(snapshots) => snapshots,
            None => return Ok(()),
        },
    };

    // a partitioned sweep is one scan per bucket, so its snapshots do not share a slot
    let Some(head_slot) = snapshots.iter().map(|snapshot| snapshot.slot).max() else {
        return Ok(());
    };
    let head_block_time = rpc.get_block_time(head_slot).await?;

    for snapshot in &snapshots {
        // the RPC already narrowed data_size/memcmp; this also applies a combined addresses clause
        if filter.as_ref().is_some_and(|f| !f.matches(snapshot)) {
            continue;
        }
        let block_time = (snapshot.slot == head_slot)
            .then_some(head_block_time)
            .flatten();
        upsert_account_snapshot(
            db,
            sse_tx,
            program.id,
            program.rpc_id,
            snapshot,
            None,
            block_time,
        )
        .await?;
    }
    Ok(())
}

/// The program scan, degraded instead of fatal: a node refusing to scan a large program falls back
/// to a partitioned sweep, and a scan that cannot be completed at all is recorded and skipped so
/// transactions and events keep indexing. `None` means "no account state this run".
async fn scan_program<R: SvmRpc + ?Sized>(
    rpc: &R,
    db: &DatabaseConnection,
    program: &programs::Model,
    address: &Pubkey,
    filter: Option<&AccountFilter>,
) -> Option<Vec<AccountSnapshot>> {
    match rpc.get_program_accounts(address, filter).await {
        Ok(snapshots) => Some(snapshots),
        Err(err) if err.is_scan_limit() => {
            partitioned_scan(rpc, db, program, address, filter).await
        }
        Err(err) => {
            tracing::warn!(
                error = %err,
                program_id = program.id,
                "Program account scan failed; skipping state catch-up."
            );
            record(
                db,
                IndexerErrorKind::Indexing,
                format!("Program account scan failed! {err}"),
                Some(program.id),
                None,
            )
            .await;
            None
        }
    }
}

/// Sweeps the program one `memcmp` bucket at a time, which keeps each pass under the node's scan
/// limit. Buckets that fail are counted rather than fatal — 250 good buckets beat none.
async fn partitioned_scan<R: SvmRpc + ?Sized>(
    rpc: &R,
    db: &DatabaseConnection,
    program: &programs::Model,
    address: &Pubkey,
    filter: Option<&AccountFilter>,
) -> Option<Vec<AccountSnapshot>> {
    let Some(offset) = program.scan_partition_offset else {
        tracing::warn!(
            program_id = program.id,
            "Program is too large to scan and has no scan_partition_offset; skipping state catch-up."
        );
        record(
            db,
            IndexerErrorKind::Indexing,
            "Program is too large for the node to scan! Set scan_partition_offset, then deactivate \
             and reactivate the program to apply it."
                .to_owned(),
            Some(program.id),
            None,
        )
        .await;
        return None;
    };
    tracing::warn!(
        program_id = program.id,
        offset,
        "Program is too large to scan; falling back to a partitioned scan."
    );

    let offset = offset as usize;
    let mut snapshots = Vec::new();
    let (mut failed, mut oversized) = (0u16, 0u16);
    for bucket in 0..=u8::MAX {
        // ANDed onto the stored descriptor rather than replacing it, so a program already narrowed
        // by data_size sweeps 256 buckets of that size class instead of 256 buckets of everything
        let mut partition = filter.cloned().unwrap_or_default();
        partition.memcmp.push(MemcmpFilter {
            offset,
            bytes: bs58::encode([bucket]).into_string(),
        });
        match scan_bucket(rpc, address, &partition).await {
            Ok(bucket_snapshots) => snapshots.extend(bucket_snapshots),
            Err(err) => {
                failed += 1;
                oversized += u16::from(err.is_scan_limit());
            }
        }
    }

    if failed > 0 {
        // the two causes need different remedies, so the count that is actionable is called out
        let remedy = if oversized > 0 {
            format!(
                " {oversized} of them stayed over the node's scan limit — pick a \
                 scan_partition_offset whose byte varies more."
            )
        } else {
            String::new()
        };
        record(
            db,
            IndexerErrorKind::Indexing,
            format!(
                "Partitioned scan incomplete: {failed}/{SCAN_BUCKETS} buckets failed at offset \
                 {offset}.{remedy}"
            ),
            Some(program.id),
            None,
        )
        .await;
    }
    Some(snapshots)
}

/// One bucket, retried once — a timeout deserves a second try, a scan-limit refusal never does
/// because the bucket is genuinely oversized and no retry shrinks it.
async fn scan_bucket<R: SvmRpc + ?Sized>(
    rpc: &R,
    address: &Pubkey,
    partition: &AccountFilter,
) -> Result<Vec<AccountSnapshot>, SvmIndexerError> {
    match rpc.get_program_accounts(address, Some(partition)).await {
        Err(err) if !err.is_scan_limit() => {
            rpc.get_program_accounts(address, Some(partition)).await
        }
        result => result,
    }
}

pub async fn stream_state<R: SvmRpc + ?Sized>(
    rpc: &R,
    db: &DatabaseConnection,
    sse_tx: &broadcast::Sender<SseEvent>,
    program: &programs::Model,
    ready: SubReady,
) -> Result<(), SvmIndexerError> {
    let address = Pubkey::try_from(program.address.as_slice()).map_err(|_| {
        SvmIndexerError::InvalidPublicKey(
            "Stored program address is not a valid pubkey!".to_owned(),
        )
    })?;
    let filter = parse_account_filters(program.account_filters.as_ref());
    let mut stream = match filter.as_ref().and_then(|f| f.address_allowlist()) {
        Some(addresses) => rpc.subscribe_accounts(&addresses, ready).await?,
        None => {
            rpc.subscribe_program(&address, filter.as_ref(), ready)
                .await?
        }
    };

    while let Some(snapshot) = stream.next().await {
        let snapshot = snapshot?;
        // predicate fallback: drop anything the server-side filter let through
        if filter.as_ref().is_some_and(|f| !f.matches(&snapshot)) {
            continue;
        }
        upsert_account_snapshot(
            db,
            sse_tx,
            program.id,
            program.rpc_id,
            &snapshot,
            None,
            None,
        )
        .await?;
    }
    // a live subscription never ends on its own; a closed stream means the connection dropped
    Err(SvmIndexerError::SubscriptionFailed(
        "account state subscription ended".to_owned(),
    ))
}
