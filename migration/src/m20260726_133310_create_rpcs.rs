use sea_orm_migration::{prelude::*};

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260726_133310_create_rpcs"
    }
}

#[derive(DeriveIden)]
enum RPCs {
    #[sea_orm(iden = "rpcs")]
    Table,
    Id,
    HttpUrl,
    WsUrl,
    CreatedAt,
    UpdatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(RPCs::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(RPCs::Id)
                            .integer()
                            .primary_key()
                            .auto_increment()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(RPCs::HttpUrl)
                            .string_len(1024_u32)
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(RPCs::WsUrl)
                            .string_len(1024_u32)
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(RPCs::CreatedAt)
                        .timestamp()
                        .not_null()
                        .default(Expr::current_timestamp())
                    )
                    .col(
                        ColumnDef::new(RPCs::UpdatedAt)
                        .timestamp()
                        .not_null()
                        .default(Expr::current_timestamp())
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(RPCs::Table).if_exists().to_owned())
            .await
    }
}
