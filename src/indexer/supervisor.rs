use std::sync::Arc;

use sea_orm::EntityTrait;
use tokio::sync::mpsc;

use crate::core::error::SvmIndexerError;
use crate::db::entities::{programs, rpcs};
use crate::indexer::Indexer;

impl Indexer {
    pub async fn start(
        self: Arc<Self>,
        mut new_program_rx: mpsc::Receiver<i32>,
    ) -> Result<(), SvmIndexerError> {
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

        // Each new program register, notifies starting its indexer here
        while let Some(program_id) = new_program_rx.recv().await {
            self.clone().spawn_by_id(program_id).await;
        }
        Ok(())
    }

    async fn spawn_by_id(self: Arc<Self>, program_id: i32) {
        let found = programs::Entity::find_by_id(program_id)
            .find_also_related(rpcs::Entity)
            .one(&self.db)
            .await;
        match found {
            Ok(Some((program, Some(rpc)))) => self.spawn_program(program, rpc).await,
            Ok(Some((_, None))) => tracing::error!(program_id, "New program has no rpc row!"),
            Ok(None) => tracing::error!(program_id, "New program vanished before indexing!"),
            Err(err) => {
                tracing::error!(program_id, error = %err, "Failed loading new program to index!")
            }
        }
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
