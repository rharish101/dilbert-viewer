//! Trait definition for a scraper
// This file is part of Dilbert Viewer.
//
// Copyright (C) 2022  Harish Rajagopal <harish.rajagopals@gmail.com>
//
// Dilbert Viewer is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Dilbert Viewer is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with Dilbert Viewer.  If not, see <https://www.gnu.org/licenses/>.
use async_trait::async_trait;
use awc::Client as HttpClient;
use log::{error, info, warn};
use sea_orm::DatabaseConnection;

use crate::errors::AppResult;

#[async_trait(?Send)]
pub trait Scraper<Data, Ref> {
    /// Retrieve cached data from the database.
    ///
    /// If data is not found in the cache, None should be returned.
    ///
    /// # Arguments:
    /// * `db` - The pool of connections to the DB
    /// * `reference` - The reference to the data that is to be retrieved
    /// * `fresh` - Whether a "fresh" cache entry is required, i.e. whether "stale" entries are to
    ///             be ignored
    async fn get_cached_data(
        &self,
        db: &Option<DatabaseConnection>,
        reference: &Ref,
        fresh: bool,
    ) -> AppResult<Option<Data>>;

    /// Cache data into the database.
    ///
    /// # Arguments:
    /// * `db` - The pool of connections to the DB
    /// * `data` - The data that is to be cached
    /// * `reference` - The reference to the data that is to be retrieved
    async fn cache_data(
        &self,
        db: &Option<DatabaseConnection>,
        data: &Data,
        reference: &Ref,
    ) -> AppResult<()>;

    /// Scrape data from the source.
    ///
    /// # Arguments:
    /// * `http_client` - The HTTP client for scraping from the source
    /// * `reference` - The reference to the data that is to be retrieved
    async fn scrape_data(&self, http_client: &HttpClient, reference: &Ref) -> AppResult<Data>;

    /// Cache data while handling exceptions.
    ///
    /// Since caching failure is not fatal, we simply log it and ignore it.
    ///
    /// # Arguments:
    /// * `db` - The pool of connections to the DB
    /// * `data` - The data that is to be cached
    /// * `reference` - The reference to the data that is to be retrieved
    async fn safely_cache_data(
        &self,
        db: &Option<DatabaseConnection>,
        data: &Data,
        reference: &Ref,
    ) {
        if let Err(err) = self.cache_data(db, data, reference).await {
            error!("{:?}", err);
        }
    }

    /// Retrieve the data, either from the source or from cache.
    ///
    /// # Arguments
    /// * `db` - The pool of connections to the DB
    /// * `http_client` - The HTTP client for scraping from the source
    /// * `reference` - The reference to the data that is to be retrieved
    async fn get_data(
        &self,
        db: &Option<DatabaseConnection>,
        http_client: &HttpClient,
        reference: &Ref,
    ) -> AppResult<Data> {
        match self.get_cached_data(db, reference, true).await {
            Ok(None) => {}
            Ok(Some(data)) => {
                info!("Successful retrieval from cache");
                return Ok(data);
            }
            Err(err) => {
                // Better to re-scrape now than crash unexpectedly, so simply log the error.
                error!("Error retrieving from cache: {}", err);
            }
        }

        info!("Couldn't fetch data from cache; trying to scrape");
        let data = match self.scrape_data(http_client, reference).await {
            Ok(data) => data,
            Err(err) => {
                // Scraping failed for some reason, so see if a "stale" cache entry is available.
                error!("Scraping failed with error: {}", err);
                return match self.get_cached_data(db, reference, false).await {
                    // No cache entry exists, so raise the scraping error.
                    Ok(None) => Err(err),

                    // Found a "stale" cache entry
                    Ok(Some(data)) => {
                        warn!(
                            "Returning stale cache entry for scraper {}",
                            std::any::type_name::<Self>()
                        );
                        Ok(data)
                    }

                    // Cache retrieval itself failed, so return this error, since the scraping
                    // error has already been logged.
                    Err(err) => Err(err),
                };
            }
        };
        info!("Scraped data from source");

        self.safely_cache_data(db, &data, reference).await;
        info!("Cached scraped data");

        Ok(data)
    }
}
