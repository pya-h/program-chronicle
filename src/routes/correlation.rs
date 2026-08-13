use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QuerySelect, QueryTrait,
    sea_query::{SelectStatement, UnionType},
};

use crate::{
    core::error::SvmIndexerError,
    db::entities::{program_accounts, program_events, transaction_accounts},
};

/// Get all registered programs participating in a transaction
pub async fn programs_for_signature(
    db: &DatabaseConnection,
    signature: &[u8],
) -> Result<Vec<i32>, SvmIndexerError> {
    let addresses: Vec<Vec<u8>> = transaction_accounts::Entity::find()
        .select_only()
        .column(transaction_accounts::Column::AccountAddress)
        .filter(transaction_accounts::Column::TxSignature.eq(signature.to_vec()))
        .into_tuple()
        .all(db)
        .await
        .map_err(|err| SvmIndexerError::db_error("Failed resolving transaction accounts!", err))?;

    let mut program_ids: Vec<i32> = if addresses.is_empty() {
        Vec::new()
    } else {
        program_accounts::Entity::find()
            .select_only()
            .column(program_accounts::Column::ProgramId)
            .distinct()
            .filter(program_accounts::Column::Address.is_in(addresses))
            .into_tuple()
            .all(db)
            .await
            .map_err(|err| SvmIndexerError::db_error("Failed resolving account owners!", err))?
    };

    let event_program_ids: Vec<i32> = program_events::Entity::find()
        .select_only()
        .column(program_events::Column::ProgramId)
        .distinct()
        .filter(program_events::Column::TxSignature.eq(signature.to_vec()))
        .into_tuple()
        .all(db)
        .await
        .map_err(|err| SvmIndexerError::db_error("Failed resolving event programs!", err))?;

    program_ids.extend(event_program_ids);
    program_ids.sort_unstable();
    program_ids.dedup();
    Ok(program_ids)
}

/// Signatures participating in `program_id`
pub fn correlated_signatures(
    program_id: i32,
    since_slot: Option<i64>,
    from_slot: Option<i64>,
    to_slot: Option<i64>,
) -> SelectStatement {
    let program_addresses = program_accounts::Entity::find()
        .select_only()
        .column(program_accounts::Column::Address)
        .filter(program_accounts::Column::ProgramId.eq(program_id))
        .into_query();

    let mut via_accounts = transaction_accounts::Entity::find()
        .select_only()
        .column(transaction_accounts::Column::TxSignature)
        .filter(transaction_accounts::Column::AccountAddress.in_subquery(program_addresses));
    if let Some(since_slot) = since_slot {
        via_accounts = via_accounts.filter(transaction_accounts::Column::Slot.gte(since_slot));
    }
    if let Some(from_slot) = from_slot {
        via_accounts = via_accounts.filter(transaction_accounts::Column::Slot.gte(from_slot));
    }
    if let Some(to_slot) = to_slot {
        via_accounts = via_accounts.filter(transaction_accounts::Column::Slot.lte(to_slot));
    }

    let mut via_events = program_events::Entity::find()
        .select_only()
        .column(program_events::Column::TxSignature)
        .filter(program_events::Column::ProgramId.eq(program_id));
    if let Some(since_slot) = since_slot {
        via_events = via_events.filter(program_events::Column::Slot.gte(since_slot));
    }
    if let Some(from_slot) = from_slot {
        via_events = via_events.filter(program_events::Column::Slot.gte(from_slot));
    }
    if let Some(to_slot) = to_slot {
        via_events = via_events.filter(program_events::Column::Slot.lte(to_slot));
    }

    let mut signatures = via_accounts.into_query();
    signatures.union(UnionType::Distinct, via_events.into_query());
    signatures
}
