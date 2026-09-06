use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260802_110000_create_transactions"
    }
}

#[derive(DeriveIden)]
enum Programs {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Slots {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Transactions {
    #[sea_orm(iden = "transactions")]
    Table,
    Id,
    Signature,
    ProgramId,
    Slot,
    SlotId,
    Success,
    Err,
    Commitment,
    Fee,
    FeePayer,
    Raw,
    CreatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Transactions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Transactions::Id)
                            .big_integer()
                            .primary_key()
                            .auto_increment()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Transactions::Signature).binary().not_null())
                    .check(Expr::cust(r#"OCTET_LENGTH("signature") = 64"#))
                    .col(ColumnDef::new(Transactions::ProgramId).integer().null())
                    .col(ColumnDef::new(Transactions::Slot).big_integer().not_null())
                    .check(Expr::col(Transactions::Slot).gte(0))
                    .col(
                        ColumnDef::new(Transactions::SlotId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Transactions::Success).boolean().not_null())
                    .col(ColumnDef::new(Transactions::Err).json_binary().null())
                    .col(
                        ColumnDef::new(Transactions::Commitment)
                            .string()
                            .not_null()
                            .default("confirmed"),
                    )
                    .col(ColumnDef::new(Transactions::Fee).big_integer().null())
                    .check(Expr::cust(r#""fee" IS NULL OR "fee" >= 0"#))
                    .col(ColumnDef::new(Transactions::FeePayer).binary().null())
                    .check(Expr::cust(
                        r#""fee_payer" IS NULL OR OCTET_LENGTH("fee_payer") = 32"#,
                    ))
                    .col(ColumnDef::new(Transactions::Raw).json_binary().not_null())
                    .col(
                        ColumnDef::new(Transactions::CreatedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-transactions-program-id")
                            .from(Transactions::Table, Transactions::ProgramId)
                            .to(Programs::Table, Programs::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-transactions-slot-id")
                            .from(Transactions::Table, Transactions::SlotId)
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
                    .name("uq-transactions-signature")
                    .table(Transactions::Table)
                    .col(Transactions::Signature)
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx-transactions-slot")
                    .table(Transactions::Table)
                    .col(Transactions::Slot)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx-transactions-fee-payer")
                    .table(Transactions::Table)
                    .col(Transactions::FeePayer)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx-transactions-commitment-slot")
                    .table(Transactions::Table)
                    .col(Transactions::Commitment)
                    .col(Transactions::Slot) // the settlement worker scans non-final rows oldest first
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Transactions::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
