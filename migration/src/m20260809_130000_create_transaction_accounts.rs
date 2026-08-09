use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260809_130000_create_transaction_accounts"
    }
}

#[derive(DeriveIden)]
enum TransactionAccounts {
    #[sea_orm(iden = "transaction_accounts")]
    Table,
    Id,
    TxSignature,
    AccountAddress,
    Slot,
    Writable,
    CreatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(TransactionAccounts::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(TransactionAccounts::Id)
                            .big_integer()
                            .primary_key()
                            .auto_increment()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(TransactionAccounts::TxSignature)
                            .binary()
                            .not_null(),
                    )
                    .check(Expr::cust(r#"OCTET_LENGTH("tx_signature") = 64"#))
                    .col(
                        ColumnDef::new(TransactionAccounts::AccountAddress)
                            .binary()
                            .not_null(),
                    )
                    .check(Expr::cust(r#"OCTET_LENGTH("account_address") = 32"#))
                    .col(
                        ColumnDef::new(TransactionAccounts::Slot)
                            .big_integer()
                            .not_null(),
                    )
                    .check(Expr::col(TransactionAccounts::Slot).gte(0))
                    .col(
                        ColumnDef::new(TransactionAccounts::Writable)
                            .boolean()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(TransactionAccounts::CreatedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;
        // one row per (tx, account) so re-ingesting a transaction is idempotent
        manager
            .create_index(
                Index::create()
                    .name("uq-transaction-accounts-tx-account")
                    .table(TransactionAccounts::Table)
                    .col(TransactionAccounts::TxSignature)
                    .col(TransactionAccounts::AccountAddress)
                    .unique()
                    .to_owned(),
            )
            .await?;
        // address → transactions reads (all mentions), newest slot first
        manager
            .create_index(
                Index::create()
                    .name("idx-transaction-accounts-address-slot")
                    .table(TransactionAccounts::Table)
                    .col(TransactionAccounts::AccountAddress)
                    .col(TransactionAccounts::Slot)
                    .to_owned(),
            )
            .await?;
        // narrow index for the tx_signature backfill, which only cares about writes
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX IF NOT EXISTS \"idx-transaction-accounts-writable-address-slot\" \
                 ON transaction_accounts (account_address, slot) WHERE writable",
            )
            .await
            .map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(TransactionAccounts::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
