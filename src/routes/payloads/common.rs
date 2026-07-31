use sea_orm::{DatabaseConnection, EntityTrait, PaginatorTrait};
use serde::Deserialize;

use crate::core::error::SvmIndexerError;

#[derive(Deserialize)]
pub struct PaginationQuery {
    pub per_page: Option<u16>,
    pub page: Option<u16>,
}

impl PaginationQuery {
    pub async fn paginate<E>(
        &self,
        query: sea_orm::Select<E>,
        db: &DatabaseConnection,
        specific_error_message: Option<&str>,
    ) -> Result<(Vec<E::Model>, u16, u16), SvmIndexerError>
    where
        E: EntityTrait,
        E::Model: sea_orm::FromQueryResult + Send + Sync,
    {
        let page = if let Some(page) = self.page {
            page.max(1)
        } else {
            1
        };
        let per_page = if let Some(per_page) = self.per_page {
            per_page.clamp(1, 100)
        } else {
            20
        };
        let paginator = query.paginate(db, per_page as u64);

        let total = paginator.num_items().await.map_err(|err| {
            SvmIndexerError::new_db_error("Failed while fetching pagination info!", err)
        })? as u16;

        let total_pages = paginator.num_pages().await.map_err(|err| {
            SvmIndexerError::new_db_error("Failed while fetching pagination info!", err)
        })? as u16;

        let items = paginator
            .fetch_page((page - 1) as u64)
            .await
            .map_err(|err| {
                SvmIndexerError::new_db_error(
                    if let Some(msg) = specific_error_message {
                        msg
                    } else {
                        "Failed fetching items!"
                    },
                    err,
                )
            })?;

        Ok((items, total, total_pages))
    }
}
