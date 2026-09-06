use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260818_140000_add_programs_backfill_frontier"
    }
}

#[derive(DeriveIden)]
enum Programs {
    Table,
    BackfillSlot,
    BackfillCursor,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Programs::Table)
                    .add_column(ColumnDef::new(Programs::BackfillSlot).big_integer().null())
                    .add_column(ColumnDef::new(Programs::BackfillCursor).binary().null())
                    .to_owned(),
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                r#"ALTER TABLE "programs" ADD CONSTRAINT "ck-programs-backfill-slot" CHECK ("backfill_slot" IS NULL OR "backfill_slot" >= 0)"#,
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                r#"ALTER TABLE "programs" ADD CONSTRAINT "ck-programs-backfill-cursor" CHECK ("backfill_cursor" IS NULL OR OCTET_LENGTH("backfill_cursor") = 64)"#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Programs::Table)
                    .drop_column(Programs::BackfillSlot)
                    .drop_column(Programs::BackfillCursor)
                    .to_owned(),
            )
            .await
    }
}
