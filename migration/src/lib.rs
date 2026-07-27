pub use sea_orm_migration::prelude::*;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260726_133310_create_rpcs::Migration),
            Box::new(m20260726_141036_create_programs::Migration),
        ]
    }
}
mod m20260726_133310_create_rpcs;
mod m20260726_141036_create_programs;
