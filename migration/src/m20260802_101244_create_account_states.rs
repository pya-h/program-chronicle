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
    TxSignature,
    WriteVersion,
    BlockTime,
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
                    .col(ColumnDef::new(AccountStates::Data).binary().null()) // Can this have actually a Null state?
                    .col(ColumnDef::new(AccountStates::Slot).big_integer().not_null())
                    .check(Expr::col(AccountStates::Slot).gt(0))
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
                            .on_delete(ForeignKeyAction::Restrict), // Or maybe SET_NULL? or NO_ACTION?
                    )
                    .to_owned(),
            )
            .await
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
