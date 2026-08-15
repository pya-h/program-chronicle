use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260802_101244_create_account_states"
    }
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
enum AccountStates {
    #[sea_orm(iden = "account_states")]
    Table,
    Id,
    ProgramAccountId,
    CreatedAt,
    UpdatedAt,

    Lamports,
    Owner,
    Data,
    Slot,
    SlotId,
    TxSignature,
    WriteVersion,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(AccountStates::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AccountStates::Id)
                            .big_integer()
                            .primary_key()
                            .auto_increment()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AccountStates::ProgramAccountId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(AccountStates::Data).binary().not_null())
                    .col(
                        ColumnDef::new(AccountStates::Lamports)
                            .big_integer()
                            .not_null(),
                    )
                    .check(Expr::col(AccountStates::Lamports).gte(0))
                    .col(ColumnDef::new(AccountStates::Owner).binary().not_null())
                    .check(Expr::cust(r#"OCTET_LENGTH("owner") = 32"#))
                    .col(ColumnDef::new(AccountStates::Slot).big_integer().not_null())
                    .check(Expr::col(AccountStates::Slot).gte(0))
                    .col(
                        ColumnDef::new(AccountStates::SlotId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AccountStates::WriteVersion)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .check(Expr::col(AccountStates::WriteVersion).gte(0))
                    .col(ColumnDef::new(AccountStates::TxSignature).binary().null()) // Check if this tx-signature can be null here, also in ProgramAccounts
                    .check(Expr::cust(
                        r#""tx_signature" IS NULL OR OCTET_LENGTH("tx_signature") = 64"#,
                    ))
                    .col(
                        ColumnDef::new(AccountStates::CreatedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(AccountStates::UpdatedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-account-states-account-id")
                            .from(AccountStates::Table, AccountStates::ProgramAccountId)
                            .to(ProgramAccounts::Table, ProgramAccounts::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-account-states-slot-id")
                            .from(AccountStates::Table, AccountStates::SlotId)
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
                    .name("uq-account-states-account-slot-write")
                    .table(AccountStates::Table)
                    .col(AccountStates::ProgramAccountId)
                    .col(AccountStates::Slot)
                    .col(AccountStates::WriteVersion)
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx-account-states-slot")
                    .table(AccountStates::Table)
                    .col(AccountStates::Slot)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx-account-states-tx-signature")
                    .table(AccountStates::Table)
                    .col(AccountStates::TxSignature)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(AccountStates::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
