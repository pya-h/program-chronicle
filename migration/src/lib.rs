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
        ]
    }
}
mod m20260726_133310_create_rpcs;
mod m20260726_141036_create_programs;
mod m20260802_083000_create_slots;
mod m20260802_084000_create_program_accounts;
mod m20260802_101244_create_account_states;
