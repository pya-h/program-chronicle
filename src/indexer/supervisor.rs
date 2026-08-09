use std::sync::Arc;

use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use tokio::sync::mpsc;

use crate::core::error::SvmIndexerError;
use crate::core::retry::retry;
use crate::core::state::ProgramCommand;
use crate::db::entities::{programs, rpcs};
use crate::indexer::Indexer;

impl Indexer {
    pub async fn start(
        self: Arc<Self>,
        mut commands: mpsc::Receiver<ProgramCommand>,
    ) -> Result<(), SvmIndexerError> {
        let programs = programs::Entity::find()
            .filter(programs::Column::Active.eq(true))
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

        // The admin API signals here to start a just-added/reactivated program or stop a deactivated one
        while let Some(command) = commands.recv().await {
            match command {
                ProgramCommand::Start(program_id) => self.clone().spawn_by_id(program_id).await,
                ProgramCommand::Stop(program_id) => self.stop_program(program_id).await,
            }
        }
        Ok(())
    }

    async fn spawn_by_id(self: Arc<Self>, program_id: i32) {
        let found = programs::Entity::find_by_id(program_id)
            .find_also_related(rpcs::Entity)
            .one(&self.db)
            .await;
        match found {
            Ok(Some((program, Some(rpc)))) if program.active => {
                self.spawn_program(program, rpc).await
            }
            Ok(Some((program, Some(_)))) => {
                tracing::debug!(program_id = program.id, "Ignoring start for inactive program.")
            }
            Ok(Some((_, None))) => tracing::error!(program_id, "New program has no rpc row!"),
            Ok(None) => tracing::error!(program_id, "New program vanished before indexing!"),
            Err(err) => {
                tracing::error!(program_id, error = %err, "Failed loading new program to index!")
            }
        }
    }

    async fn spawn_program(self: Arc<Self>, program: programs::Model, rpc: rpcs::Model) {
        let program_id = program.id;
        let mut handles = self.handles.lock().await;
        if handles.get(&program_id).is_some_and(|handle| !handle.is_finished()) {
            return;
        }
        let client = self.client_for(&rpc).await;
        let indexer = self.clone();
        let policy = self.retry_policy;
        let handle = tokio::spawn(async move {
            let result = retry(policy, "program-indexer", || {
                indexer.run_program(program.clone(), client.clone())
            })
            .await;
            if let Err(err) = result {
                tracing::error!(program_id, error = %err, "Program indexer gave up after retries!");
            }
        });
        handles.insert(program_id, handle);
    }

    async fn stop_program(&self, program_id: i32) {
        if let Some(handle) = self.handles.lock().await.remove(&program_id) {
            handle.abort();
            tracing::info!(program_id, "Deactivated program; stopped its indexer task.");
        }
    }
}
