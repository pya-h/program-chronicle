use std::collections::HashMap;

use axum::{Router, extract::State, routing::get};
use sea_orm::{
    ConnectionTrait, DatabaseBackend, EntityTrait, PaginatorTrait, QueryOrder, Statement,
};

use crate::{
    core::{
        error::SvmIndexerError,
        response::{ApiResponse, FinalResponse},
        state::AppState,
    },
    db::entities::{programs, rpcs},
    routes::responses::stats::{IndexerStatsResponse, ProgramProgressResponse, TableEstimates},
};

async fn get_stats_handler(State(state): State<AppState>) -> FinalResponse<IndexerStatsResponse> {
    let programs_count = programs::Entity::find()
        .count(&state.db)
        .await
        .map_err(|err| SvmIndexerError::db_error("Failed counting programs!", err))?;
    let rpcs_count = rpcs::Entity::find()
        .count(&state.db)
        .await
        .map_err(|err| SvmIndexerError::db_error("Failed counting rpcs!", err))?;

    // large tables use the planner estimate (near-instant) instead of an exact COUNT(*)
    let rows = state
        .db
        .query_all_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            "SELECT relname, GREATEST(reltuples, 0)::bigint AS estimate FROM pg_class \
             WHERE relname IN ('slots', 'transactions', 'program_events', 'program_accounts')",
        ))
        .await
        .map_err(|err| SvmIndexerError::db_error("Failed reading table estimates!", err))?;

    let mut estimates: HashMap<String, i64> = HashMap::new();
    for row in rows {
        let relname: String = row
            .try_get("", "relname")
            .map_err(|err| SvmIndexerError::db_error("Failed reading table estimates!", err))?;
        let estimate: i64 = row
            .try_get("", "estimate")
            .map_err(|err| SvmIndexerError::db_error("Failed reading table estimates!", err))?;
        estimates.insert(relname, estimate);
    }
    let estimate_of = |table: &str| estimates.get(table).copied().unwrap_or(0);

    let phases = match &state.phases {
        Some(phases) => phases.lock().await.clone(),
        None => HashMap::new(),
    };
    let per_program = programs::Entity::find()
        .order_by_asc(programs::Column::Id)
        .all(&state.db)
        .await
        .map_err(|err| SvmIndexerError::db_error("Failed fetching program progress!", err))?
        .into_iter()
        .map(|program| {
            let phase = phases.get(&program.id).copied();
            ProgramProgressResponse::from_model(program, phase)
        })
        .collect();

    Ok(ApiResponse::ok(IndexerStatsResponse {
        programs: programs_count,
        rpcs: rpcs_count,
        estimates: TableEstimates {
            slots: estimate_of("slots"),
            transactions: estimate_of("transactions"),
            events: estimate_of("program_events"),
            accounts: estimate_of("program_accounts"),
        },
        per_program,
    }))
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/", get(get_stats_handler))
}
