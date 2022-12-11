//! Scraper to get info on the latest Dilbert comic
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
use std::rc::Rc;

use async_trait::async_trait;
use awc::http::StatusCode;
use chrono::{Duration, NaiveDate, NaiveDateTime};
use log::{debug, error, info};
#[cfg(test)]
use mockall::automock;
use serde::{Deserialize, Serialize};

use crate::client::HttpClient;
use crate::constants::{LATEST_DATE_REFRESH, SRC_COMIC_PREFIX, SRC_DATE_FMT};
use crate::db::{RedisPool, SerdeAsyncCommands};
use crate::errors::{AppError, AppResult};
use crate::scrapers::Scraper;
use crate::utils::{curr_date, curr_datetime};

/// Key for storing the latest date in the DB
const LATEST_DATE_KEY: &str = "latest-date";

/// Values stored for the latest date
#[derive(Deserialize, Serialize)]
struct LatestDateInfo {
    date: NaiveDate,
    last_check: NaiveDateTime,
}

/// Struct to scrape the date of the latest Dilbert comic.
///
/// This scraper returns that date.
pub struct LatestDateScraper<T: RedisPool> {
    db: Option<T>,
    http_client: Rc<HttpClient>,
}

#[cfg_attr(test, automock)]
impl<T: RedisPool + 'static> LatestDateScraper<T> {
    /// Initialize a latest date scraper.
    pub fn new(db: Option<T>, http_client: Rc<HttpClient>) -> Self {
        Self { db, http_client }
    }

    /// Retrieve the date of the latest comic.
    #[cfg_attr(test, allow(dead_code))]
    pub async fn get_latest_date(&self) -> AppResult<NaiveDate> {
        self.get_data(&()).await
    }

    /// Update the latest date in the cache.
    ///
    /// # Arguments
    /// * `date` - The date of the latest comic
    #[cfg_attr(test, allow(dead_code))]
    pub async fn update_latest_date(&self, date: &NaiveDate) -> AppResult<()> {
        self.cache_data(date, &()).await
    }
}

#[async_trait(?Send)]
impl<T: RedisPool> Scraper<NaiveDate, ()> for LatestDateScraper<T> {
    /// Get the cached latest date from the database.
    ///
    /// In the rare case that the latest date entry wasn't found in the cache, None is returned.
    /// The boolean return flag indicates whether the cache entry is stale (i.e. it was updated a
    /// long time back) or not.
    async fn get_cached_data(&self, _ref: &()) -> AppResult<Option<(NaiveDate, bool)>> {
        let mut conn = if let Some(db) = &self.db {
            db.get().await?
        } else {
            return Ok(None);
        };

        // Render enforces an upper limit on the DB memory, and we manually set it to evict the
        // least recently used keys. Thus, since it's possible that this key has been evicted, we
        // use an `Option` for `raw_data`.
        let info: Option<LatestDateInfo> = conn.get(LATEST_DATE_KEY).await?;

        Ok(if let Some(info) = info {
            debug!("Retrieved info from DB for latest date");
            // The latest date is fresh if it has been updated within the last
            // `LATEST_DATE_REFRESH` hours.
            let last_fresh_time = curr_datetime() - Duration::hours(LATEST_DATE_REFRESH);
            Some((info.date, info.last_check >= last_fresh_time))
        } else {
            None
        })
    }

    /// Cache the latest date into the database.
    async fn cache_data(&self, date: &NaiveDate, _ref: &()) -> AppResult<()> {
        let mut conn = if let Some(db) = &self.db {
            db.get().await?
        } else {
            return Ok(());
        };

        let new_info = LatestDateInfo {
            date: date.to_owned(),
            last_check: curr_datetime(),
        };
        conn.set(LATEST_DATE_KEY, &new_info).await?;

        info!("Successfully updated latest date in cache to: {}", date);
        Ok(())
    }

    /// Scrape the date of the latest comic from "dilbert.com".
    async fn scrape_data(&self, _ref: &()) -> AppResult<NaiveDate> {
        // If there is no comic for this date yet, "dilbert.com" will auto-redirect to the
        // homepage.
        let today = curr_date();
        let path = format!("{}{}", SRC_COMIC_PREFIX, curr_date().format(SRC_DATE_FMT));

        info!("Trying date \"{}\" for latest comic", today);
        let mut resp = self.http_client.get(&path).send().await?;
        let status = resp.status();

        match status {
            StatusCode::FOUND => {
                // Redirected to homepage, implying that there's no comic for this date. There must
                // be a comic for the previous date, so use that.
                let date = today - Duration::days(1);
                info!("No comic found for today ({}); using date: {}", today, date);
                Ok(date)
            }
            StatusCode::OK => {
                info!("Found comic for today ({}); using it as latest date", today);
                Ok(today)
            }
            _ => {
                error!("Unexpected response status: {}", status);
                Err(AppError::Scrape(format!(
                    "Couldn't scrape latest date: {:#?}",
                    resp.body().await?
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use actix_web::http::{Method, StatusCode};
    use chrono::{DateTime, Utc};
    use deadpool_redis::redis::{Cmd, Value};
    use redis_test::{IntoRedisValue, MockCmd, MockRedisConnection};
    use test_case::test_case;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    use crate::db::mock::MockPool;
    use crate::scrapers::scraper::mock::GetCacheState;
    use crate::utils::{curr_date, mock::mock_time_file};

    #[test_case(GetCacheState::Fresh; "fresh retrieval")]
    #[test_case(GetCacheState::Stale; "stale retrieval")]
    #[test_case(GetCacheState::NotFound; "empty cache")]
    #[actix_web::test]
    /// Test cache retrieval of the latest date.
    ///
    /// # Arguments
    /// * `status` - Status for the cache retrieval
    async fn test_latest_date_cache_retrieval(status: GetCacheState) {
        // Set up the expected return values, and the entry to store in the mock cache.
        let date = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
        let (expected, cache_entry) = match status {
            GetCacheState::Fresh => {
                let latest_date_info = LatestDateInfo {
                    date,
                    last_check: curr_datetime(), // Should be recent enough.
                };
                (Some((date, true)), Some(latest_date_info))
            }
            GetCacheState::Stale => {
                let latest_date_info = LatestDateInfo {
                    date,
                    last_check: curr_datetime() - Duration::days(1), // Should be old enough.
                };
                (Some((date, false)), Some(latest_date_info))
            }
            GetCacheState::NotFound => (None, None),
            GetCacheState::Fail => panic!("Invalid test parameter"),
        };

        // Set up the mock Redis command that the scraper is expected to request.
        let cache_key =
            serde_json::to_vec(LATEST_DATE_KEY).expect("Couldn't serialize mock cache key");
        let cache_value = if let Some(value) = cache_entry {
            serde_json::to_vec(&value)
                .expect("Couldn't serialize mock cache value")
                .into_redis_value()
        } else {
            Value::Nil
        };
        let retrieval_cmd = MockCmd::new(Cmd::get(cache_key), Ok(cache_value));

        // Max pool size is one, since only one connection is needed.
        let db = MockPool::new(1);
        if let Err((_, err)) = db.add(MockRedisConnection::new([retrieval_cmd])).await {
            panic!("Couldn't add mock DB connection to mock DB pool: {}", err);
        };

        // The HTTP client shouldn't be used, so make the base URL empty.
        let http_client = Rc::new(HttpClient::new(String::new()));
        let scraper = LatestDateScraper::new(Some(db), http_client);
        let result = scraper
            .get_cached_data(&())
            .await
            .expect("Failed to get latest date from cache");
        assert_eq!(
            result, expected,
            "Retrieved the wrong latest date from cache"
        );
    }

    #[actix_web::test]
    /// Test cache storage of the latest date.
    async fn test_latest_date_cache_storage() {
        // Set up the entry to store in the mock cache.
        let latest_date_info = LatestDateInfo {
            date: NaiveDate::from_ymd_opt(2000, 1, 1).unwrap(),
            last_check: curr_datetime(),
        };

        // Set up the mock Redis command that the scraper is expected to request.
        let cache_key =
            serde_json::to_vec(LATEST_DATE_KEY).expect("Couldn't serialize mock cache key");
        let cache_value =
            serde_json::to_vec(&latest_date_info).expect("Couldn't serialize mock cache value");
        let storage_cmd = MockCmd::new(Cmd::set(cache_key, cache_value), Ok(Value::Okay));

        // Max pool size is one, since only one connection is needed.
        let db = MockPool::new(1);
        if let Err((_, err)) = db.add(MockRedisConnection::new([storage_cmd])).await {
            panic!("Couldn't add mock DB connection to mock DB pool: {}", err);
        };

        // Mock the datetime with the expected `last_check` time, otherwise a different one will be
        // used by the scraper during cache storage.
        let faketime_file = mock_time_file(DateTime::from_utc(latest_date_info.last_check, Utc));
        faketime::enable(&faketime_file);

        // The HTTP client shouldn't be used, so make the base URL empty.
        let http_client = Rc::new(HttpClient::new(String::new()));
        let scraper = LatestDateScraper::new(Some(db), http_client);
        scraper
            .cache_data(&latest_date_info.date, &())
            .await
            .expect("Failed to set latest date in cache");
    }

    #[test_case(true; "is latest")]
    #[test_case(false; "is not latest")]
    #[actix_web::test]
    /// Test scraping of the latest date.
    ///
    /// # Arguments
    /// * `is_latest` - Whether the current date is to be indicated as the date of the latest comic
    async fn test_latest_date_scraping(is_latest: bool) {
        let mock_server = MockServer::start().await;
        let http_client = Rc::new(HttpClient::new(mock_server.uri()));
        let date = curr_date();

        // The DB shouldn't be used, so use a pool with no connections.
        let db = Some(MockPool::new(0));
        let scraper = LatestDateScraper::new(db, http_client);

        let expected = if is_latest {
            date
        } else {
            // "dilbert.com" releases a new comic every day. Hence, if today's comic doesn't exist,
            // then it must exist for yesterday.
            date - Duration::days(1)
        };

        let date_str = date.format(SRC_DATE_FMT).to_string();
        let response_status = if is_latest {
            StatusCode::OK
        } else {
            // "dilbert.com" uses 302 FOUND to inform that the comic doesn't exist.
            StatusCode::FOUND
        };

        // Set up the mock server to return the pre-fetched "dilbert.com" response for the given date.
        Mock::given(method(Method::GET.as_str()))
            .and(path(format!("/{}{}", SRC_COMIC_PREFIX, date_str)))
            // Response body shouldn't matter, so keep it empty.
            .respond_with(ResponseTemplate::new(response_status.as_u16()))
            .mount(&mock_server)
            .await;

        // The scraping should fail if and only if the server redirects.
        let result = scraper
            .scrape_data(&())
            .await
            .expect("Failed to scrape latest date");
        assert_eq!(result, expected, "Scraped the wrong latest date");
    }
}
