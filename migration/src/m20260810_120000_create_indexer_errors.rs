use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260810_120000_create_indexer_errors"
    }
}

#[derive(DeriveIden)]
enum Programs {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum IndexerErrors {
    #[sea_orm(iden = "indexer_errors")]
    Table,
    Id,
    Kind,
    Message,
    ProgramId,
    Slot,
    Seen,
    CreatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(IndexerErrors::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(IndexerErrors::Id)
                            .big_integer()
                            .primary_key()
                            .auto_increment()
                            .not_null(),
                    )
                    .col(ColumnDef::new(IndexerErrors::Kind).string().not_null())
                    .col(ColumnDef::new(IndexerErrors::Message).text().not_null())
                    .col(ColumnDef::new(IndexerErrors::ProgramId).integer().null())
                    .col(ColumnDef::new(IndexerErrors::Slot).big_integer().null())
                    .check(Expr::col(IndexerErrors::Slot).gte(0))
                    .col(
                        ColumnDef::new(IndexerErrors::Seen)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(IndexerErrors::CreatedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-indexer-errors-program-id")
                            .from(IndexerErrors::Table, IndexerErrors::ProgramId)
                            .to(Programs::Table, Programs::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx-indexer-errors-seen-created-at")
                    .table(IndexerErrors::Table)
                    .col(IndexerErrors::Seen)
                    .col(IndexerErrors::CreatedAt)
                    .to_owned(),
            )
            .await
            .map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(IndexerErrors::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
