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
use deadpool_redis::Pool as RedisPool;
use log::{error, info, warn};

use crate::errors::AppResult;

#[async_trait(?Send)]
pub trait Scraper<Data, Ref> {
    /// Retrieve cached data from the database.
    ///
    /// If data is not found in the cache, None should be returned. Otherwise, this returns the
    /// cached data, and a boolean indicating whether the entry is "fresh" (doesn't need to be
    /// updated) or not.
    ///
    /// # Arguments:
    /// * `db` - The pool of connections to the DB
    /// * `reference` - The reference to the data that is to be retrieved
    async fn get_cached_data(
        &self,
        db: &Option<RedisPool>,
        reference: &Ref,
    ) -> AppResult<Option<(Data, bool)>>;

    /// Cache data into the database.
    ///
    /// # Arguments:
    /// * `db` - The pool of connections to the DB
    /// * `data` - The data that is to be cached
    /// * `reference` - The reference to the data that is to be retrieved
    async fn cache_data(
        &self,
        db: &Option<RedisPool>,
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
    async fn safely_cache_data(&self, db: &Option<RedisPool>, data: &Data, reference: &Ref) {
        if let Err(err) = self.cache_data(db, data, reference).await {
            error!("Error caching data: {}", err);
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
        db: &Option<RedisPool>,
        http_client: &HttpClient,
        reference: &Ref,
    ) -> AppResult<Data> {
        let stale_data = match self.get_cached_data(db, reference).await {
            Ok(Some((data, true))) => {
                info!("Successful retrieval from cache");
                return Ok(data);
            }
            Ok(Some((data, false))) => Some(data),
            Ok(None) => None,
            Err(err) => {
                // Better to re-scrape now than crash unexpectedly, so simply log the error.
                error!("Error retrieving from cache: {}", err);
                None
            }
        };

        info!("Couldn't fetch fresh data from cache; trying to scrape");
        let err = match self.scrape_data(http_client, reference).await {
            Ok(data) => {
                info!("Scraped data from source");
                self.safely_cache_data(db, &data, reference).await;
                info!("Cached scraped data");
                return Ok(data);
            }
            Err(err) => err,
        };

        // Scraping failed for some reason, so use the "stale" cache entry, if available.
        error!("Scraping failed with error: {}", err);

        return match stale_data {
            // No stale cache entry exists, so raise the scraping error.
            None => Err(err),

            // Return the "stale" cache entry
            Some(data) => {
                warn!(
                    "Returning stale cache entry for scraper {}",
                    std::any::type_name::<Self>()
                );
                Ok(data)
            }
        };
    }
}
