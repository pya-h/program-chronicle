use std::sync::Arc;
use std::time::Duration;

use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep};

use crate::core::error::SvmIndexerError;
use crate::core::indexer_error::{IndexerErrorKind, record};
use crate::core::retry::retry;
use crate::core::state::{ProgramCommand, ProgramPhase};
use crate::db::entities::programs;
use crate::indexer::Indexer;

impl Indexer {
    pub async fn start(
        self: Arc<Self>,
        mut commands: mpsc::Receiver<ProgramCommand>,
    ) -> Result<(), SvmIndexerError> {
        let programs = programs::Entity::find()
            .filter(programs::Column::Active.eq(true))
            .all(&self.db)
            .await
            .map_err(|err| SvmIndexerError::db_error("Failed loading programs to index!", err))?;

        for program in programs {
            self.clone().spawn_program(program.id).await;
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
        let found = programs::Entity::find_by_id(program_id).one(&self.db).await;
        match found {
            Ok(Some(program)) if program.active => self.spawn_program(program.id).await,
            Ok(Some(program)) => {
                tracing::debug!(
                    program_id = program.id,
                    "Ignoring start for inactive program."
                )
            }
            Ok(None) => tracing::error!(program_id, "New program vanished before indexing!"),
            Err(err) => {
                tracing::error!(program_id, error = %err, "Failed loading new program to index!")
            }
        }
    }

    async fn spawn_program(self: Arc<Self>, program_id: i32) {
        let mut handles = self.handles.lock().await;
        if handles
            .get(&program_id)
            .is_some_and(|handle| !handle.is_finished())
        {
            return;
        }
        let indexer = self.clone();
        let policy = self.retry_policy;
        let base_cooldown = self.restart_cooldown;
        let cooldown_cap = self.restart_cooldown_cap;

        let handle = tokio::spawn(async move {
            let mut cooldown = base_cooldown;
            loop {
                let started = Instant::now();
                match retry(policy, "program-indexer", || {
                    indexer.run_program(program_id)
                })
                .await
                {
                    Ok(()) => {
                        tracing::warn!(
                            program_id,
                            "Program indexer run ended; restarting after cooldown."
                        )
                    }
                    Err(err) => {
                        tracing::error!(program_id, error = %err, "Program indexer failed rapidly; restarting after cooldown.");
                        record(
                            &indexer.db,
                            IndexerErrorKind::Indexing,
                            err.to_string(),
                            Some(program_id),
                            None,
                        )
                        .await;
                    }
                }
                indexer
                    .set_phase(program_id, ProgramPhase::Restarting)
                    .await;
                // a run that stayed healthy past the reset window earns a fresh cooldown
                let healthy = started.elapsed() >= policy.reset_after;
                sleep(cooldown).await;
                cooldown = if healthy {
                    base_cooldown
                } else {
                    cooldown
                        .checked_mul(2)
                        .map_or(cooldown_cap, |next| next.min(cooldown_cap))
                };
            }
        });
        handles.insert(program_id, handle);
    }

    async fn stop_program(&self, program_id: i32) {
        if let Some(handle) = self.handles.lock().await.remove(&program_id) {
            handle.abort();
            self.phases.lock().await.remove(&program_id);
            tracing::info!(program_id, "Deactivated program; stopped its indexer task.");
        }
    }

    /// Level-triggered safety net: every tick, re-spawn any active program whose task is missing or finished
    pub async fn run_reconciler(self: Arc<Self>, interval: Duration) {
        let mut ticker = tokio::time::interval(interval);
        ticker.tick().await; // consume the immediate first tick; boot already spawned everything
        loop {
            ticker.tick().await;
            self.clone().reconcile_once().await;
        }
    }

    async fn reconcile_once(self: Arc<Self>) {
        let found = programs::Entity::find()
            .filter(programs::Column::Active.eq(true))
            .all(&self.db)
            .await;
        let programs = match found {
            Ok(programs) => programs,
            Err(err) => {
                return tracing::error!(error = %err, "Reconcile pass failed loading programs!");
            }
        };
        for program in programs {
            let program_id = program.id;
            let stopped = {
                let handles = self.handles.lock().await;
                handles
                    .get(&program_id)
                    .is_none_or(|handle| handle.is_finished())
            };
            if !stopped {
                continue; // its running or cooling down
            }
            tracing::warn!(
                program_id,
                "Found an active program which wasn't being indexed as expected! Respawing..."
            );
            record(
                &self.db,
                IndexerErrorKind::Indexing,
                "indexer task was not running; reconciler respawned it".to_owned(),
                Some(program_id),
                None,
            )
            .await;
            self.clone().spawn_program(program_id).await;
        }
    }
}
