pub mod backfill;
pub mod events;
pub mod filter;
pub mod persist;
pub mod rpc;
pub mod state;
pub mod supervisor;
pub mod transactions;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use sea_orm::{
    ActiveModelTrait,
    ActiveValue::{Set, Unchanged},
    ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    sea_query::Expr,
};
use solana_sdk::commitment_config::CommitmentConfig;
use tokio::{
    sync::{Mutex, Semaphore, broadcast, oneshot},
    task::JoinHandle,
};

use crate::core::commitment::TransactionCommitment;
use crate::core::error::SvmIndexerError;
use crate::core::indexer_error::{IndexerErrorKind, record};
use crate::core::retry::RetryPolicy;
use crate::core::sse::SseEvent;
use crate::core::state::{ProgramPhase, ProgramRuntime, ProgramRuntimes};
use crate::db::entities::{programs, rpcs};
use crate::indexer::rpc::SvmRpc;
use crate::indexer::rpc::json_rpc::JsonRpc;
use crate::indexer::state::{catch_up_state, stream_state};
use crate::indexer::transactions::{catch_up_transactions, stream_transactions};

pub struct Indexer {
    db: DatabaseConnection,
    /// Ingest level for catch-up and streams. Writes are idempotent do-nothing with no reorg
    /// overwrite: under `confirmed` the latest state self-heals on the next change, but a reorged
    /// slot's rows may linger until a finalized-reconcile pass.
    commitment: CommitmentConfig,
    sse_tx: broadcast::Sender<SseEvent>,
    clients: Mutex<HashMap<i32, Arc<dyn SvmRpc>>>,
    handles: Mutex<HashMap<i32, JoinHandle<()>>>,
    runtimes: ProgramRuntimes,
    /// Bounds how many programs run their bulk catch-up at once
    catchup_semaphore: Semaphore,
    retry_policy: RetryPolicy,
    restart_cooldown: Duration,
    restart_cooldown_cap: Duration,
    cursor_interval: Duration,
}

impl Indexer {
    #[allow(clippy::too_many_arguments)] // tuning knobs, each read straight from its own env var
    pub fn new(
        db: DatabaseConnection,
        commitment: CommitmentConfig,
        sse_tx: broadcast::Sender<SseEvent>,
        catchup_concurrency: usize,
        max_retries: u32,
        restart_cooldown: Duration,
        restart_cooldown_cap: Duration,
        cursor_interval: Duration,
    ) -> Self {
        Self {
            db,
            commitment,
            sse_tx,
            clients: Mutex::new(HashMap::new()),
            handles: Mutex::new(HashMap::new()),
            runtimes: ProgramRuntimes::default(),
            catchup_semaphore: Semaphore::new(catchup_concurrency),
            retry_policy: RetryPolicy::new(max_retries),
            restart_cooldown,
            restart_cooldown_cap,
            cursor_interval,
        }
    }

    /// One `SvmRpc` client per network, built lazily from the `rpcs` row and shared across its  programs' tasks.
    async fn client_for(&self, rpc: &rpcs::Model) -> Arc<dyn SvmRpc> {
        let mut clients = self.clients.lock().await;
        if let Some(client) = clients.get(&rpc.id) {
            return client.clone();
        }
        let client: Arc<dyn SvmRpc> = Arc::new(JsonRpc::new(
            rpc.http_url.clone(),
            rpc.ws_url.clone(),
            self.commitment,
        ));
        clients.insert(rpc.id, client.clone());
        client
    }

    pub fn runtimes(&self) -> ProgramRuntimes {
        self.runtimes.clone()
    }

    async fn set_phase(&self, program_id: i32, phase: ProgramPhase) {
        self.runtimes
            .lock()
            .await
            .entry(program_id)
            .and_modify(|runtime| runtime.phase = phase)
            .or_insert_with(|| ProgramRuntime::new(phase));
    }

    /// Only touches a program that is already running: a head with no task behind it would report
    /// progress on something that stopped moving.
    async fn set_head(&self, program_id: i32, head: i64) {
        if let Some(runtime) = self.runtimes.lock().await.get_mut(&program_id) {
            runtime.head_slot = Some(head);
        }
    }

    fn ingest_commitment(&self) -> TransactionCommitment {
        TransactionCommitment::from_ingest(self.commitment)
    }

    pub fn ingests_confirmed(&self) -> bool {
        self.commitment == CommitmentConfig::confirmed()
    }

    async fn run_program(&self, program_id: i32) -> Result<(), SvmIndexerError> {
        let (program, network) = self.load_program(program_id).await?;
        let rpc = self.client_for(&network).await;
        let commitment = self.ingest_commitment();
        let head = rpc.get_slot().await? as i64;
        let program = self.resolve_start_slot(program, head).await?;
        self.set_phase(program.id, ProgramPhase::Backfilling).await;
        self.set_head(program.id, head).await;
        {
            let _permit = self
                .catchup_semaphore
                .acquire()
                .await
                .expect("catch-up semaphore is never closed");
            // stays ahead of the streams: inside `try_join!` a dropped stream would cancel this walk
            // mid-way, and it would restart from the same floor on every retry, never finishing
            catch_up_transactions(
                rpc.as_ref(),
                &self.db,
                &self.sse_tx,
                &program,
                commitment,
                None,
            )
            .await?; // backfill the downtime gap
        } // release the permit before streaming — steady-state is unthrottled
        self.advance_cursor(&program, head).await?;
        self.set_phase(program.id, ProgramPhase::Syncing).await;

        let (state_ready, state_live) = oneshot::channel();
        let (tx_ready, tx_live) = oneshot::channel();
        tokio::try_join!(
            stream_state(rpc.as_ref(), &self.db, &self.sse_tx, &program, state_ready),
            stream_transactions(
                rpc.as_ref(),
                &self.db,
                &self.sse_tx,
                &program,
                commitment,
                tx_ready
            ),
            async {
                self.close_subscribe_gap(
                    rpc.as_ref(),
                    &program,
                    commitment,
                    head,
                    state_live,
                    tx_live,
                )
                .await?;
                self.track_cursor(rpc.as_ref(), &program, commitment, head)
                    .await
            },
        )?; // try_join so when one stream ends/errors the other stops too; the caller
        // (spawn_program) then retries the whole run with backoff, then cools down and restarts.
        Ok(())
    }

    /// Catchup the walk only pages backwards from where it started, and the streams only carry
    /// what the node accepted them for. So this closes the gap.
    async fn close_subscribe_gap(
        &self,
        rpc: &dyn SvmRpc,
        program: &programs::Model,
        commitment: TransactionCommitment,
        head: i64,
        state_live: oneshot::Receiver<()>,
        tx_live: oneshot::Receiver<()>,
    ) -> Result<(), SvmIndexerError> {
        // an error instead of readiness leaves the sender dropped; the stream reports it, so the
        // only thing to do here is stop waiting and let `try_join!` unwind
        let _ = state_live.await;
        {
            let _permit = self
                .catchup_semaphore
                .acquire()
                .await
                .expect("catch-up semaphore is never closed");
            catch_up_state(rpc, &self.db, &self.sse_tx, program).await?;
        }

        let _ = tx_live.await;
        catch_up_transactions(rpc, &self.db, &self.sse_tx, program, commitment, Some(head)).await?;

        self.set_phase(program.id, ProgramPhase::Live).await;
        Ok(())
    }

    /// Keeps the cursor moving for the life of a live run. Each tick re-walks `[cursor, newest]`
    /// *before* moving the cursor, so a notification the live stream dropped is repaired instead of
    /// skipped past; advancing on its own would cement the loss.
    async fn track_cursor(
        &self,
        rpc: &dyn SvmRpc,
        program: &programs::Model,
        commitment: TransactionCommitment,
        head: i64,
    ) -> Result<(), SvmIndexerError> {
        // the run's own model goes stale the moment `advance_cursor` writes, so the walk's floor is
        // tracked here rather than read back from `program.last_processed_slot`
        let mut cursor = head;
        let mut ticking = true;
        loop {
            tokio::time::sleep(self.cursor_interval).await;
            let next = match self
                .walk_and_advance(rpc, program, commitment, cursor)
                .await
            {
                Ok(next) => {
                    ticking = true;
                    next
                }
                Err(err) => {
                    tracing::warn!(
                        error = %err,
                        program_id = program.id,
                        "Cursor tick failed; retrying on the next interval."
                    );
                    // the run keeps reporting `Live`, so a stalled cursor is invisible unless it is
                    // recorded here — once per streak, not once per tick
                    if ticking {
                        record(
                            &self.db,
                            IndexerErrorKind::Indexing,
                            format!("Cursor stopped advancing on a live run! {err}"),
                            Some(program.id),
                            None,
                        )
                        .await;
                        ticking = false;
                    }
                    continue;
                }
            };
            cursor = next;
        }
    }

    async fn walk_and_advance(
        &self,
        rpc: &dyn SvmRpc,
        program: &programs::Model,
        commitment: TransactionCommitment,
        cursor: i64,
    ) -> Result<i64, SvmIndexerError> {
        let head = rpc.get_slot().await? as i64;
        self.set_head(program.id, head).await;
        catch_up_transactions(
            rpc,
            &self.db,
            &self.sse_tx,
            program,
            commitment,
            Some(cursor),
        )
        .await?;
        self.advance_cursor(program, head).await?;
        Ok(head)
    }

    async fn load_program(
        &self,
        program_id: i32,
    ) -> Result<(programs::Model, rpcs::Model), SvmIndexerError> {
        let found = programs::Entity::find_by_id(program_id)
            .find_also_related(rpcs::Entity)
            .one(&self.db)
            .await
            .map_err(|err| SvmIndexerError::db_error("Failed loading program to index!", err))?;
        match found {
            Some((program, Some(network))) => Ok((program, network)),
            Some(_) => Err(SvmIndexerError::ItemNotFound(
                "Program points at an rpc that no longer exists!".to_owned(),
            )),
            None => Err(SvmIndexerError::ItemNotFound(
                "Program vanished before indexing it!".to_owned(),
            )),
        }
    }

    async fn resolve_start_slot(
        &self,
        program: programs::Model,
        head: i64,
    ) -> Result<programs::Model, SvmIndexerError> {
        if program.start_slot.is_some() {
            return Ok(program);
        }
        programs::ActiveModel {
            id: Unchanged(program.id),
            start_slot: Set(Some(head)), // start_slot being NULL means start from rn
            ..Default::default()
        }
        .update(&self.db)
        .await
        .map_err(|err| SvmIndexerError::db_error("Failed pinning program start slot!", err))
    }

    async fn advance_cursor(
        &self,
        program: &programs::Model,
        slot: i64,
    ) -> Result<(), SvmIndexerError> {
        let now = chrono::Utc::now().naive_utc();
        programs::Entity::update_many()
            .col_expr(programs::Column::LastProcessedSlot, Expr::value(slot))
            .col_expr(programs::Column::LastProcessedAt, Expr::value(now))
            .filter(programs::Column::Id.eq(program.id))
            .exec(&self.db)
            .await
            .map_err(|err| SvmIndexerError::db_error("Failed advancing program cursor!", err))?;
        Ok(())
    }
}
