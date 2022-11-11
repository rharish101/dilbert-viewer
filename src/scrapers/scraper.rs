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
use std::borrow::Borrow;

use async_trait::async_trait;
use awc::Client as HttpClient;
use log::{error, info};
use sea_orm::DatabaseConnection;

use crate::errors::AppResult;

#[async_trait(?Send)]
pub trait Scraper<Data, DataBorrowed: Sync + ?Sized, Ref: Sync + ?Sized>
where
    // This allows using &str instead of &String, when `Data` is String.
    Data: Send + Sync + Borrow<DataBorrowed>,
{
    /// Retrieve cached data from the database.
    ///
    /// If data is not found in the cache, None should be returned.
    ///
    /// # Arguments:
    /// * `db` - The pool of connections to the DB
    /// * `reference` - The reference to the data that is to be retrieved
    async fn get_cached_data(
        &self,
        db: &Option<DatabaseConnection>,
        reference: &Ref,
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
        data: &DataBorrowed,
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
        data: &DataBorrowed,
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
    /// * `reference` - The thing that uniquely identifies the data that is requested, i.e. a
    ///                 reference to the requested data
    async fn get_data(
        &self,
        db: &Option<DatabaseConnection>,
        http_client: &HttpClient,
        reference: &Ref,
    ) -> AppResult<Data> {
        match self.get_cached_data(db, reference).await {
            Ok(None) => {}
            Ok(Some(data)) => {
                info!("Successful retrieval from cache");
                return Ok(data);
            }
            Err(err) => {
                // Better to re-scrape now than crash unexpectedly, so simply log the error.
                error!("{:?}", err);
            }
        }

        info!("Couldn't fetch data from cache; trying to scrape");
        let data = self.scrape_data(http_client, reference).await?;
        info!("Scraped data from source");

        self.safely_cache_data(db, data.borrow(), reference).await;
        info!("Cached scraped data");

        Ok(data)
    }
}
