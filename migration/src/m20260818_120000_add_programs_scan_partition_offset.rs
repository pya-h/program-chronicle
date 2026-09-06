use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260818_120000_add_programs_scan_partition_offset"
    }
}

#[derive(DeriveIden)]
enum Programs {
    Table,
    ScanPartitionOffset,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Programs::Table)
                    // byte position a refused program-wide scan is split on; NULL = no workaround
                    // available, so the scan is skipped instead of retried forever
                    .add_column(
                        ColumnDef::new(Programs::ScanPartitionOffset)
                            .integer()
                            .null(),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                r#"ALTER TABLE "programs" ADD CONSTRAINT "ck-programs-scan-partition-offset" CHECK ("scan_partition_offset" >= 0)"#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Programs::Table)
                    .drop_column(Programs::ScanPartitionOffset)
                    .to_owned(),
            )
            .await
    }
}
