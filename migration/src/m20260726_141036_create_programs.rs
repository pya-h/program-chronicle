use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260726_141036_create_programs"
    }
}

#[derive(DeriveIden)]
enum Programs {
    Table,
    Id,
    RpcId,
    Address,
    Name,
    LastProcessedSlot,
    LastProcessedAt,
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
                    .table(Programs::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Programs::Id)
                            .primary_key()
                            .integer()
                            .auto_increment()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Programs::RpcId).integer().not_null())
                    .col(ColumnDef::new(Programs::Address).binary().not_null())
                    .check(Expr::cust(r#"OCTET_LENGTH("address") = 32"#))
                    .col(ColumnDef::new(Programs::Name).string_len(64).null())
                    .col(ColumnDef::new(Programs::LastProcessedAt).timestamp().null())
                    .col(
                        ColumnDef::new(Programs::LastProcessedSlot)
                            .big_integer()
                            .default(0),
                    )
                    .check(Expr::col(Programs::LastProcessedSlot).gte(0))
                    .col(
                        ColumnDef::new(Programs::CreatedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(Programs::UpdatedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-programs-rpc-id")
                            .from(Programs::Table, Programs::RpcId)
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
                    .name("uq-programs-address-rpc-id")
                    .table(Programs::Table)
                    .col(Programs::Address)
                    .col(Programs::RpcId)
                    .unique() // preventing the same program on same network being indexed again!
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Programs::Table).if_exists().to_owned())
            .await
    }
}
