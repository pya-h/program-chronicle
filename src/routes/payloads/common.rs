use sea_orm::{DatabaseConnection, PaginatorTrait, SelectorTrait};
use serde::{Deserialize, Deserializer, de};

use crate::core::error::SvmIndexerError;

// serde_urlencoded delivers `#[serde(flatten)]` fields as strings, so a bare `Option<u16>` fails to coerce (`invalid type: string`). 
fn de_opt_u16<'de, D>(deserializer: D) -> Result<Option<u16>, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<String>::deserialize(deserializer)? {
        Some(raw) if !raw.trim().is_empty() => {
            raw.trim().parse::<u16>().map(Some).map_err(de::Error::custom)
        }
        _ => Ok(None),
    }
}

pub(crate) fn de_opt_bool<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<String>::deserialize(deserializer)? {
        Some(raw) if !raw.trim().is_empty() => {
            raw.trim().parse::<bool>().map(Some).map_err(de::Error::custom)
        }
        _ => Ok(None),
    }
}

#[derive(Deserialize)]
pub struct PaginationQuery {
    #[serde(default, deserialize_with = "de_opt_u16")]
    pub per_page: Option<u16>,
    #[serde(default, deserialize_with = "de_opt_u16")]
    pub page: Option<u16>,
}

impl PaginationQuery {
    pub async fn paginate<'db, Q>(
        &self,
        query: Q,
        db: &'db DatabaseConnection,
        specific_error_message: Option<&str>,
    ) -> Result<
        (
            Vec<<<Q as PaginatorTrait<'db, DatabaseConnection>>::Selector as SelectorTrait>::Item>,
            u64,
            u64,
        ),
        SvmIndexerError,
    >
    where
        Q: PaginatorTrait<'db, DatabaseConnection>,
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
            SvmIndexerError::db_error("Failed while fetching pagination info!", err)
        })?;

        let total_pages = paginator.num_pages().await.map_err(|err| {
            SvmIndexerError::db_error("Failed while fetching pagination info!", err)
        })?;

        let items = paginator
            .fetch_page((page - 1) as u64)
            .await
            .map_err(|err| {
                SvmIndexerError::db_error(
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

    pub async fn paginate_or_all<'db, Q>(
        &self,
        query: Q,
        db: &'db DatabaseConnection,
        specific_error_message: Option<&str>,
    ) -> Result<
        (
            Vec<<<Q as PaginatorTrait<'db, DatabaseConnection>>::Selector as SelectorTrait>::Item>,
            u64,
            u64,
        ),
        SvmIndexerError,
    >
    where
        Q: PaginatorTrait<'db, DatabaseConnection>,
    {
        // either param signals intent to paginate; `paginate` fills the missing side with its default
        // (page 1 / 20-per-page, clamped). only a request with neither returns the whole set.
        if self.per_page.is_some() || self.page.is_some() {
            return self.paginate(query, db, specific_error_message).await;
        }
        let items = query
            .paginate(db, u32::MAX as u64) // one page wide enough to hold the whole result set
            .fetch_page(0)
            .await
            .map_err(|err| {
                SvmIndexerError::db_error(
                    specific_error_message.unwrap_or("Failed fetching items!"),
                    err,
                )
            })?;
        let total = items.len() as u64;
        Ok((items, total, 1))
    }
}
