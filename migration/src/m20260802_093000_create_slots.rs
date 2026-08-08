use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260802_093000_create_slots"
    }
}

#[derive(DeriveIden)]
enum Slots {
    Table,
    Slot,
    BlockTime,
    CreatedAt,
    UpdatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Slots::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Slots::Slot)
                            .big_integer()
                            .primary_key()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Slots::BlockTime).big_integer().null())
                    .check(Expr::cust(r#""block_time" IS NULL OR "block_time" >= 0"#))
                    .col(
                        ColumnDef::new(Slots::CreatedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(Slots::UpdatedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Slots::Table).if_exists().to_owned())
            .await
    }
}
