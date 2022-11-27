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
use log::{error, info, warn};

use crate::errors::AppResult;

#[async_trait(?Send)]
pub trait Scraper<D, R> {
    /// Retrieve cached data from the database.
    ///
    /// If data is not found in the cache, None should be returned. Otherwise, this returns the
    /// cached data, and a boolean indicating whether the entry is "fresh" (doesn't need to be
    /// updated) or not.
    ///
    /// # Arguments:
    /// * `reference` - The reference to the data that is to be retrieved
    async fn get_cached_data(&self, reference: &R) -> AppResult<Option<(D, bool)>>;

    /// Cache data into the database.
    ///
    /// # Arguments:
    /// * `data` - The data that is to be cached
    /// * `reference` - The reference to the data that is to be retrieved
    async fn cache_data(&self, data: &D, reference: &R) -> AppResult<()>;

    /// Scrape data from the source.
    ///
    /// # Arguments:
    /// * `reference` - The reference to the data that is to be retrieved
    async fn scrape_data(&self, reference: &R) -> AppResult<D>;

    /// Cache data while handling exceptions.
    ///
    /// Since caching failure is not fatal, we simply log it and ignore it.
    ///
    /// # Arguments:
    /// * `data` - The data that is to be cached
    /// * `reference` - The reference to the data that is to be retrieved
    async fn safely_cache_data(&self, data: &D, reference: &R) {
        if let Err(err) = self.cache_data(data, reference).await {
            error!("Error caching data: {}", err);
        }
    }

    /// Retrieve the data, either from the source or from cache.
    ///
    /// # Arguments
    /// * `reference` - The reference to the data that is to be retrieved
    async fn get_data(&self, reference: &R) -> AppResult<D> {
        let stale_data = match self.get_cached_data(reference).await {
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
        let err = match self.scrape_data(reference).await {
            Ok(data) => {
                info!("Scraped data from source");
                self.safely_cache_data(&data, reference).await;
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

#[cfg(test)]
pub mod mock {
    /// Enum for the state of the mock struct during cache retrieval.
    pub enum GetCacheState {
        /// Retrieve a fresh value.
        Fresh,
        /// Retrieve a stale value.
        Stale,
        /// Value not found in cache.
        NotFound,
        /// Retrieval crashes.
        Fail,
    }
}

#[cfg(test)]
mod tests {
    use super::mock::GetCacheState;
    use super::*;

    use mockall::mock;
    use test_case::test_case;

    use crate::errors::AppError;

    mock! {
        TestScraper {}

        #[async_trait(?Send)]
        impl Scraper<i32, ()> for TestScraper {
            async fn get_cached_data(&self, reference: &()) -> AppResult<Option<(i32, bool)>>;
            async fn cache_data(&self, data: &i32, reference: &()) -> AppResult<()>;
            async fn scrape_data(&self, reference: &()) -> AppResult<i32>;
        }
    }

    #[test_case(GetCacheState::Fresh, true, true; "fresh retrieval")]
    #[test_case(GetCacheState::Stale, true, true; "stale retrieval, scrape works, storage works")]
    #[test_case(GetCacheState::Stale, true, false; "stale retrieval, scrape works, storage fails")]
    #[test_case(GetCacheState::Stale, false, true; "stale retrieval, scrape fails")]
    #[test_case(GetCacheState::NotFound, true, true; "empty cache, storage works")]
    #[test_case(GetCacheState::NotFound, true, false; "empty cache, storage fails")]
    #[test_case(GetCacheState::Fail, true, true; "cache retrieval fails, storage works")]
    #[test_case(GetCacheState::Fail, true, false; "cache retrieval fails, storage fails")]
    #[actix_web::test]
    /// Test multiple scenarios of data requested from a scraper using the trait's provided method.
    ///
    /// # Arguments
    /// * `retrieve_status` - Status for the cache retrieval
    /// * `scrape_works` - Whether scraping works
    /// * `storage_works` - Whether cache storage works
    async fn test_scraper_get_data(
        retrieve_status: GetCacheState,
        scrape_works: bool,
        storage_works: bool,
    ) {
        let expected = 1;
        let mut mock_scraper = MockTestScraper::new();

        // Mock cache retrieval.
        mock_scraper
            .expect_get_cached_data()
            .return_once(move |_| match retrieve_status {
                GetCacheState::Fresh => Ok(Some((expected, true))),
                GetCacheState::Stale => Ok(Some((expected, false))),
                GetCacheState::NotFound => Ok(None),
                GetCacheState::Fail => Err(AppError::Internal("Manual error".into())),
            });

        // Mock cache storage.
        mock_scraper.expect_cache_data().return_once(move |_, _| {
            if storage_works {
                Ok(())
            } else {
                Err(AppError::Internal("Manual error".into()))
            }
        });

        // Mock scraping.
        mock_scraper.expect_scrape_data().return_once(move |_| {
            if scrape_works {
                Ok(expected)
            } else {
                Err(AppError::Internal("Manual error".into()))
            }
        });

        let result = mock_scraper
            .get_data(&())
            .await
            .expect("Data retrieval from scraper crashed");
        assert_eq!(result, expected, "Scraper returned the wrong data");
    }
}
