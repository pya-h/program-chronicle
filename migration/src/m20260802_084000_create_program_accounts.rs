use std::u64;

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260802_084000_create_program_accounts"
    }
}

#[derive(DeriveIden)]
enum ProgramAccounts {
    #[sea_orm(iden = "program_accounts")]
    Table,
    Id,
    ProgramId,
    Address,
    CreatedAt,
    UpdatedAt,

    // Next ones are mostly for caching, to return the account state fast instead of querying thousands of account states.
    Lamports,
    Owner,
    State,
    LatestSlot,
    LatestTxSignature,
    LatestWriteVersion,
    LatestBlockTime,
}

#[derive(DeriveIden)]
enum Programs {
    Table,
    Id,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ProgramAccounts::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ProgramAccounts::Id)
                            .big_integer()
                            .primary_key()
                            .auto_increment()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ProgramAccounts::ProgramId)
                            .integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(ProgramAccounts::Address).binary().not_null()).check(Expr::cust(r#"OCTET_LENGTH("address") = 32"#))
                    .col(ColumnDef::new(ProgramAccounts::CreatedAt).timestamp().not_null().default(Expr::current_timestamp()))
                    .col(ColumnDef::new(ProgramAccounts::UpdatedAt).timestamp().not_null().default(Expr::current_timestamp()))

                    .col(ColumnDef::new(ProgramAccounts::Lamports).big_integer().not_null()).check(Expr::col(ProgramAccounts::Lamports).gte(0))
                    .col(ColumnDef::new(ProgramAccounts::Owner).binary().not_null()).check(Expr::cust(r#"OCTET_LENGTH("owner") = 32"#))
                    .col(ColumnDef::new(ProgramAccounts::State).binary().not_null())
                    .col(
                        ColumnDef::new(ProgramAccounts::LatestSlot)
                            .big_integer()
                            .not_null(),
                    )
                    .check(Expr::col(ProgramAccounts::LatestSlot).gte(0))
                    .col(
                        ColumnDef::new(ProgramAccounts::LatestTxSignature)
                            .binary()
                            .null(),
                    )
                    .check(Expr::cust(r#""latest_tx_signature" IS NULL OR OCTET_LENGTH("latest_tx_signature") = 64"#))
                    .col(ColumnDef::new(ProgramAccounts::LatestWriteVersion).big_integer().not_null().default(0))
                    .check(Expr::col(ProgramAccounts::LatestWriteVersion).gte(0))
                    .col(ColumnDef::new(ProgramAccounts::LatestBlockTime).big_integer().null())
                    .check(Expr::cust(r#""latest_block_time" IS NULL OR "latest_block_time" >= 0"#))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-accounts-program-id")
                            .from(ProgramAccounts::Table, ProgramAccounts::ProgramId)
                            .to(Programs::Table, Programs::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Restrict), // Or maybe SET_NULL? or NO_ACTION?
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq-accounts-address-program-id")
                    .table(ProgramAccounts::Table)
                    .col(ProgramAccounts::Address)
                    .col(ProgramAccounts::ProgramId)
                    .unique()
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(ProgramAccounts::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
