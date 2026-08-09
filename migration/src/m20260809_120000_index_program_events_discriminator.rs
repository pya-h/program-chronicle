use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260809_120000_index_program_events_discriminator"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // the query predicate must stay byte-identical to this expression or the planner skips the index
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX IF NOT EXISTS \"idx-program-events-program-discriminator-slot\" \
                 ON program_events (program_id, substr(data, 1, 8), slot)",
            )
            .await
            .map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "DROP INDEX IF EXISTS \"idx-program-events-program-discriminator-slot\"",
            )
            .await
            .map(|_| ())
    }
}
