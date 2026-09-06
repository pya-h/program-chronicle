use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260901_120000_add_slots_status"
    }
}

#[derive(DeriveIden)]
enum Slots {
    Table,
    Status,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Slots::Table)
                    .add_column(
                        ColumnDef::new(Slots::Status)
                            .string()
                            .not_null()
                            .default("unknown"),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Slots::Table)
                    .drop_column(Slots::Status)
                    .to_owned(),
            )
            .await
    }
}
