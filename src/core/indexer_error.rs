use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection};

use crate::db::entities::indexer_errors;

#[derive(Clone, Copy, Debug)]
pub enum IndexerErrorKind {
    Database,
    Scheduling,
    Indexing,
}

impl IndexerErrorKind {
    fn as_str(self) -> &'static str {
        match self {
            IndexerErrorKind::Database => "database",
            IndexerErrorKind::Scheduling => "scheduling",
            IndexerErrorKind::Indexing => "indexing",
        }
    }
}

// Best-effort: this runs on a failure path, so a failed insert is logged rather than propagated.
pub async fn record(
    db: &DatabaseConnection,
    kind: IndexerErrorKind,
    message: String,
    program_id: Option<i32>,
    slot: Option<i64>,
) {
    let row = indexer_errors::ActiveModel {
        kind: Set(kind.as_str().to_owned()),
        message: Set(message),
        program_id: Set(program_id),
        slot: Set(slot),
        ..Default::default()
    };
    if let Err(err) = row.insert(db).await {
        tracing::error!(error = %err, "Failed recording an indexer error!");
    }
}
