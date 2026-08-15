use sea_orm::{
    ActiveValue::Set,
    ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
    TransactionTrait,
    sea_query::{Expr, OnConflict},
};
use tokio::sync::broadcast;

use crate::core::error::SvmIndexerError;
use crate::core::sse::SseEvent;
use crate::db::entities::{program_accounts, slots};
use crate::indexer::rpc::AccountSnapshot;

pub async fn ensure_slot<C: ConnectionTrait>(
    db: &C,
    rpc_id: i32,
    slot: i64,
    block_time: Option<i64>,
) -> Result<i64, SvmIndexerError> {
    let now = chrono::Utc::now().naive_utc();
    let row = slots::Entity::insert(slots::ActiveModel {
        rpc_id: Set(rpc_id),
        slot: Set(slot),
        block_time: Set(block_time),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    })
    .on_conflict(
        OnConflict::columns([slots::Column::RpcId, slots::Column::Slot])
            // write-once block_time: keep it once known, fill it only while NULL; return the id regardless
            .value(
                slots::Column::BlockTime,
                Expr::cust(r#"COALESCE("slots"."block_time", EXCLUDED."block_time")"#),
            )
            .value(
                slots::Column::UpdatedAt,
                Expr::cust(
                    r#"CASE WHEN "slots"."block_time" IS NULL AND EXCLUDED."block_time" IS NOT NULL
                         THEN CURRENT_TIMESTAMP ELSE "slots"."updated_at" END"#,
                ),
            )
            .to_owned(),
    )
    .exec_with_returning(db)
    .await
    .map_err(|err| SvmIndexerError::db_error("Failed ensuring slot dimension row!", err))?;
    Ok(row.id)
}

// `tx_sig` is only supplied by Geyser. Json rpc doesnt provide that; In future we handle that with a workaround.
pub async fn upsert_account_snapshot(
    db: &DatabaseConnection,
    sse_tx: &broadcast::Sender<SseEvent>,
    program_id: i32,
    rpc_id: i32,
    snapshot: &AccountSnapshot,
    tx_sig: Option<Vec<u8>>,
    block_time: Option<i64>,
) -> Result<(), SvmIndexerError> {
    let now = chrono::Utc::now().naive_utc();
    let address = snapshot.address.to_bytes().to_vec();
    let owner = snapshot.owner.to_bytes().to_vec();
    let lamports = snapshot.lamports as i64;
    let slot = snapshot.slot as i64;
    let write_version = snapshot.write_version.unwrap_or(0) as i64;

    let txn = db
        .begin()
        .await
        .map_err(|err| SvmIndexerError::db_error("Failed opening snapshot transaction!", err))?;

    let slot_id = ensure_slot(&txn, rpc_id, slot, block_time).await?;

    let existing = program_accounts::Entity::find()
        .filter(program_accounts::Column::ProgramId.eq(program_id))
        .filter(program_accounts::Column::Address.eq(address.clone()))
        .one(&txn)
        .await
        .map_err(|err| SvmIndexerError::db_error("Failed loading account!", err))?;

    // value-dedupe: the unique key can't catch a reboot's catch-up (its slot differs), so compare
    // by value to skip a redundant rewrite of an unchanged account on every restart
    if let Some(row) = &existing
        && row.state == snapshot.data
        && row.lamports == lamports
        && row.owner == owner
    {
        txn.commit()
            .await
            .map_err(|err| SvmIndexerError::db_error("Failed committing snapshot!", err))?;
        return Ok(());
    }

    let program_account_id = match existing {
        Some(row) => row.id,
        None => {
            program_accounts::Entity::insert(program_accounts::ActiveModel {
                program_id: Set(program_id),
                address: Set(address),
                state: Set(snapshot.data.clone()),
                owner: Set(owner.clone()),
                lamports: Set(lamports),
                rent_epoch: Set(Some(snapshot.rent_epoch as i64)),
                latest_slot: Set(slot),
                latest_slot_id: Set(slot_id),
                latest_tx_signature: Set(tx_sig.clone()),
                latest_write_version: Set(write_version),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            })
            .on_conflict(
                // the snapshot and the live stream can first-see an account at the same moment; a
                // no-op update keeps RETURNING yielding the row the loser needs to carry on with
                OnConflict::columns([
                    program_accounts::Column::Address,
                    program_accounts::Column::ProgramId,
                ])
                .value(
                    program_accounts::Column::UpdatedAt,
                    Expr::cust(r#""program_accounts"."updated_at""#),
                )
                .to_owned(),
            )
            .exec_with_returning(&txn)
            .await
            .map_err(|err| SvmIndexerError::db_error("Failed creating account!", err))?
            .id
        }
    };

    let advanced = program_accounts::Entity::update_many()
        .col_expr(
            program_accounts::Column::State,
            Expr::value(snapshot.data.clone()),
        )
        .col_expr(program_accounts::Column::Owner, Expr::value(owner))
        .col_expr(program_accounts::Column::Lamports, Expr::value(lamports))
        .col_expr(program_accounts::Column::LatestSlot, Expr::value(slot))
        .col_expr(program_accounts::Column::LatestSlotId, Expr::value(slot_id))
        .col_expr(
            program_accounts::Column::LatestTxSignature,
            Expr::value(tx_sig),
        )
        .col_expr(
            program_accounts::Column::LatestWriteVersion,
            Expr::value(write_version),
        )
        .col_expr(program_accounts::Column::UpdatedAt, Expr::value(now))
        .filter(program_accounts::Column::Id.eq(program_account_id))
        .filter(
            Condition::any()
                .add(program_accounts::Column::LatestSlot.lt(slot))
                .add(
                    Condition::all()
                        .add(program_accounts::Column::LatestSlot.eq(slot))
                        // `lte`, not `lt`: a same-slot json-rpc rewrite (write_version 0 == 0) must
                        // still advance to the newer value
                        .add(program_accounts::Column::LatestWriteVersion.lte(write_version)),
                ),
        )
        .exec(&txn)
        .await
        .map_err(|err| SvmIndexerError::db_error("Failed advancing account!", err))?;

    txn.commit()
        .await
        .map_err(|err| SvmIndexerError::db_error("Failed committing snapshot!", err))?;

    // a rejected out-of-order write advances no row; don't broadcast its stale state
    if advanced.rows_affected > 0 {
        let _ = sse_tx.send(SseEvent::account_changed(
            program_id,
            snapshot.address,
            slot,
            write_version,
            snapshot.data.clone(),
        ));
    }
    Ok(())
}
