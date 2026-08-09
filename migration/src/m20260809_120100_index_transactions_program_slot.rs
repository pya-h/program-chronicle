use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260809_120100_index_transactions_program_slot"
    }
}

#[derive(DeriveIden)]
enum Transactions {
    #[sea_orm(iden = "transactions")]
    Table,
    ProgramId,
    Slot,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx-transactions-program-slot")
                    .table(Transactions::Table)
                    .col(Transactions::ProgramId)
                    .col(Transactions::Slot)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .if_exists()
                    .name("idx-transactions-program-slot")
                    .table(Transactions::Table)
                    .to_owned(),
            )
            .await
    }
}
