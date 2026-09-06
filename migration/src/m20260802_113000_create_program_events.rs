use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260802_113000_create_program_events"
    }
}

#[derive(DeriveIden)]
enum Programs {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum ProgramAccounts {
    #[sea_orm(iden = "program_accounts")]
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Slots {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum ProgramEvents {
    #[sea_orm(iden = "program_events")]
    Table,
    Id,
    ProgramId,
    ProgramAccountId,
    TxSignature,
    Slot,
    SlotId,
    LogIndex,
    Data,
    CreatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ProgramEvents::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ProgramEvents::Id)
                            .big_integer()
                            .primary_key()
                            .auto_increment()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ProgramEvents::ProgramId)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ProgramEvents::ProgramAccountId)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(ProgramEvents::TxSignature)
                            .binary()
                            .not_null(),
                    )
                    .check(Expr::cust(r#"OCTET_LENGTH("tx_signature") = 64"#))
                    .col(ColumnDef::new(ProgramEvents::Slot).big_integer().not_null())
                    .check(Expr::col(ProgramEvents::Slot).gte(0))
                    .col(
                        ColumnDef::new(ProgramEvents::SlotId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(ProgramEvents::LogIndex).integer().not_null())
                    .check(Expr::col(ProgramEvents::LogIndex).gte(0))
                    .col(ColumnDef::new(ProgramEvents::Data).binary().not_null())
                    .col(
                        ColumnDef::new(ProgramEvents::CreatedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-program-events-program-id")
                            .from(ProgramEvents::Table, ProgramEvents::ProgramId)
                            .to(Programs::Table, Programs::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-program-events-account-id")
                            .from(ProgramEvents::Table, ProgramEvents::ProgramAccountId)
                            .to(ProgramAccounts::Table, ProgramAccounts::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-program-events-slot-id")
                            .from(ProgramEvents::Table, ProgramEvents::SlotId)
                            .to(Slots::Table, Slots::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq-program-events-tx-signature-log-index")
                    .table(ProgramEvents::Table)
                    .col(ProgramEvents::TxSignature)
                    .col(ProgramEvents::LogIndex)
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx-program-events-program-slot")
                    .table(ProgramEvents::Table)
                    .col(ProgramEvents::ProgramId)
                    .col(ProgramEvents::Slot)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx-program-events-account")
                    .table(ProgramEvents::Table)
                    .col(ProgramEvents::ProgramAccountId)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(ProgramEvents::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
