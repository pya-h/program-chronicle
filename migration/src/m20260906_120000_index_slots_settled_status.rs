use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260906_120000_index_slots_settled_status"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // partial on purpose: `unknown` and `canonical` are almost every row
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX IF NOT EXISTS \"idx-slots-settled-status\" ON slots (status) \
                 WHERE status IN ('reorged', 'pruned')",
            )
            .await
            .map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP INDEX IF EXISTS \"idx-slots-settled-status\"")
            .await
            .map(|_| ())
    }
}
