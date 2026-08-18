pub use sea_orm_migration::prelude::*;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260726_133310_create_rpcs::Migration),
            Box::new(m20260726_141036_create_programs::Migration),
            Box::new(m20260802_083000_create_slots::Migration),
            Box::new(m20260802_084000_create_program_accounts::Migration),
            Box::new(m20260802_101244_create_account_states::Migration),
            Box::new(m20260802_110000_create_transactions::Migration),
            Box::new(m20260802_113000_create_program_events::Migration),
            Box::new(m20260809_120000_index_program_events_discriminator::Migration),
            Box::new(m20260809_120100_index_transactions_program_slot::Migration),
            Box::new(m20260809_130000_create_transaction_accounts::Migration),
            Box::new(m20260810_120000_create_indexer_errors::Migration),
            Box::new(m20260814_120000_add_track_account_states_to_programs::Migration),
        ]
    }
}
mod m20260726_133310_create_rpcs;
mod m20260726_141036_create_programs;
mod m20260802_083000_create_slots;
mod m20260802_084000_create_program_accounts;
mod m20260802_101244_create_account_states;
mod m20260802_110000_create_transactions;
mod m20260802_113000_create_program_events;
mod m20260809_120000_index_program_events_discriminator;
mod m20260809_120100_index_transactions_program_slot;
mod m20260809_130000_create_transaction_accounts;
mod m20260810_120000_create_indexer_errors;
mod m20260814_120000_add_track_account_states_to_programs;
