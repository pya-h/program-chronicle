pub mod backfill;
pub mod events;
pub mod persist;
pub mod rpc;
pub mod state;
pub mod supervisor;
pub mod transactions;

use std::collections::HashMap;
use std::sync::Arc;

use sea_orm::{
    ActiveModelTrait,
    ActiveValue::{Set, Unchanged},
    ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, sea_query::Expr,
};
use solana_sdk::commitment_config::CommitmentConfig;
use tokio::{
    sync::{Mutex, Semaphore, broadcast},
    task::JoinHandle,
};

use crate::core::error::SvmIndexerError;
use crate::core::sse::SseEvent;
use crate::db::entities::{programs, rpcs};
use crate::indexer::rpc::SvmRpc;
use crate::indexer::rpc::json_rpc::JsonRpc;
use crate::indexer::state::{catch_up_state, stream_state};
use crate::indexer::transactions::{catch_up_transactions, stream_transactions};


pub struct Indexer {
    db: DatabaseConnection,
    /// Ingest level for catch-up and streams. Writes are idempotent do-nothing with no reorg
    /// overwrite: under `confirmed` the latest cache self-heals on the next change, but a reorged
    /// slot's history row may linger until a finalized-reconcile pass.
    commitment: CommitmentConfig,
    sse_tx: broadcast::Sender<SseEvent>,
    clients: Mutex<HashMap<i32, Arc<dyn SvmRpc>>>,
    handles: Mutex<HashMap<i32, JoinHandle<()>>>,
    /// Bounds how many programs run their bulk catch-up at once, so a boot or registration burst
    /// meters backfill instead of stampeding the pool and the RPC. Live streaming is unthrottled.
    catchup_semaphore: Semaphore,
}

impl Indexer {
    pub fn new(
        db: DatabaseConnection,
        commitment: CommitmentConfig,
        sse_tx: broadcast::Sender<SseEvent>,
        catchup_concurrency: usize,
    ) -> Self {
        Self {
            db,
            commitment,
            sse_tx,
            clients: Mutex::new(HashMap::new()),
            handles: Mutex::new(HashMap::new()),
            catchup_semaphore: Semaphore::new(catchup_concurrency),
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

    async fn run_program(
        &self,
        program: programs::Model,
        rpc: Arc<dyn SvmRpc>,
    ) -> Result<(), SvmIndexerError> {
        let head = rpc.get_slot().await? as i64;
        let program = self.resolve_start_slot(program, head).await?;
        {
            let _permit = self
                .catchup_semaphore
                .acquire()
                .await
                .expect("catch-up semaphore is never closed");
            catch_up_state(rpc.as_ref(), &self.db, &self.sse_tx, &program).await?; // cache up to the head
            catch_up_transactions(rpc.as_ref(), &self.db, &self.sse_tx, &program).await?; // backfill the downtime gap
        } // release the permit before streaming — steady-state is unthrottled
        self.advance_cursor(&program, head).await?;

        tokio::try_join!(
            stream_state(rpc.as_ref(), &self.db, &self.sse_tx, &program),
            stream_transactions(rpc.as_ref(), &self.db, &self.sse_tx, &program),
        )?; // try_join so when one ends/stops, the other one stops too. When the retry mechanism is implemented,
        // The end/stop of one leads to stopping both and retrying both again.
        Ok(())
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
