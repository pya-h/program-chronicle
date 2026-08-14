use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260814_120000_add_track_account_states_to_programs"
    }
}

#[derive(DeriveIden)]
enum Programs {
    Table,
    TrackAccountStates,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Programs::Table)
                    // existing programs keep today's behavior: history stays on unless explicitly disabled
                    .add_column(
                        ColumnDef::new(Programs::TrackAccountStates)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Programs::Table)
                    .drop_column(Programs::TrackAccountStates)
                    .to_owned(),
            )
            .await
    }
}
