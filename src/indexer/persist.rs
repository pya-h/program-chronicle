use sea_orm::{
    ActiveValue::Set, ConnectionTrait, EntityTrait,
    sea_query::{Expr, OnConflict},
};

use crate::core::error::SvmIndexerError;
use crate::db::entities::slots;

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
