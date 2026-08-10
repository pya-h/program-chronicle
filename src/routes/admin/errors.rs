use axum::{
    Router,
    extract::{Path, Query, State},
    response::NoContent,
    routing::{get, patch},
};
use sea_orm::{
    ColumnTrait, EntityTrait, QueryFilter, QueryOrder, sea_query::Expr,
};

use crate::{
    core::{
        error::SvmIndexerError,
        response::{ApiResponse, FinalResponse, NoContentResponse},
        state::AppState,
    },
    db::entities::indexer_errors,
    routes::{
        payloads::admin::IndexerErrorsQuery, responses::admin::GetIndexerErrorsResponse,
    },
};

async fn get_errors_handler(
    State(state): State<AppState>,
    Query(query): Query<IndexerErrorsQuery>,
) -> FinalResponse<GetIndexerErrorsResponse> {
    let mut errors_query = indexer_errors::Entity::find();
    if let Some(seen) = query.seen {
        errors_query = errors_query.filter(indexer_errors::Column::Seen.eq(seen));
    }
    // unseen first, then newest
    let errors_query = errors_query
        .order_by_asc(indexer_errors::Column::Seen)
        .order_by_desc(indexer_errors::Column::CreatedAt)
        .order_by_desc(indexer_errors::Column::Id);

    let (errors, total, pages) = query
        .pagination
        .paginate_or_all(errors_query, &state.db, Some("Failed fetching indexer errors!"))
        .await?;
    Ok(ApiResponse::ok(GetIndexerErrorsResponse::new(errors, total, pages)))
}

async fn mark_error_seen_handler(
    State(state): State<AppState>,
    Path(error_id): Path<i64>,
) -> NoContentResponse {
    let result = indexer_errors::Entity::update_many()
        .col_expr(indexer_errors::Column::Seen, Expr::value(true))
        .filter(indexer_errors::Column::Id.eq(error_id))
        .exec(&state.db)
        .await
        .map_err(|err| {
            SvmIndexerError::db_error_with_id(
                "Failed marking the indexer error seen!",
                err,
                error_id as u64,
            )
        })?;
    if result.rows_affected == 0 {
        return Err(SvmIndexerError::ItemNotFound("No such indexer error!".to_owned()));
    }
    Ok(NoContent)
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(get_errors_handler))
        .route("/{error_id}", patch(mark_error_seen_handler))
}
