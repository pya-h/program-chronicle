use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260802_083000_create_slots"
    }
}

#[derive(DeriveIden)]
enum Slots {
    Table,
    Id,
    RpcId,
    Slot,
    BlockTime,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum RPCs {
    #[sea_orm(iden = "rpcs")]
    Table,
    Id,
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
                        ColumnDef::new(Slots::Id)
                            .big_integer()
                            .primary_key()
                            .auto_increment()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Slots::RpcId).integer().not_null())
                    // slot is only unique within a network, so it is scoped by rpc_id, not a PK.
                    .col(ColumnDef::new(Slots::Slot).big_integer().not_null())
                    .check(Expr::col(Slots::Slot).gte(0))
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
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-slots-rpc-id")
                            .from(Slots::Table, Slots::RpcId)
                            .to(RPCs::Table, RPCs::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq-slots-rpc-id-slot")
                    .table(Slots::Table)
                    .col(Slots::RpcId)
                    .col(Slots::Slot)
                    .unique()
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Slots::Table).if_exists().to_owned())
            .await
    }
}
