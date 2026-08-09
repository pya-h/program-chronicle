use std::sync::Arc;

use sea_orm::EntityTrait;

use crate::core::error::SvmIndexerError;
use crate::db::entities::{programs, rpcs};
use crate::indexer::Indexer;

impl Indexer {
    pub async fn start(self: Arc<Self>) -> Result<(), SvmIndexerError> {
        let programs = programs::Entity::find()
            .find_also_related(rpcs::Entity)
            .all(&self.db)
            .await
            .map_err(|err| SvmIndexerError::db_error("Failed loading programs to index!", err))?;

        for (program, rpc) in programs {
            let Some(rpc) = rpc else {
                tracing::error!(program_id = program.id, "Skipping program with no rpc row!");
                continue;
            };
            self.clone().spawn_program(program, rpc).await;
        }
        Ok(())
    }

    async fn spawn_program(self: Arc<Self>, program: programs::Model, rpc: rpcs::Model) {
        let program_id = program.id;
        let client = self.client_for(&rpc).await;
        let indexer = self.clone();
        let handle = tokio::spawn(async move {
            if let Err(err) = indexer.run_program(program, client).await {
                tracing::error!(program_id, error = %err, "Program indexer task exited!");
            }
        });
        self.handles.lock().await.insert(program_id, handle);
    }
}
