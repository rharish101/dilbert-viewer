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
use async_trait::async_trait;
use awc::{http::StatusCode, Client as HttpClient};
use chrono::{Duration, NaiveDate, NaiveDateTime};
use deadpool_redis::{redis::AsyncCommands, Pool as RedisPool};
use log::{debug, error, info};
use serde::{Deserialize, Serialize};

use crate::constants::{LATEST_DATE_REFRESH, SRC_DATE_FMT, SRC_PREFIX};
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
pub struct LatestDateScraper {}

impl LatestDateScraper {
    /// Initialize a latest date scraper.
    pub fn new() -> Self {
        Self {}
    }

    /// Retrieve the date of the latest comic.
    ///
    /// # Arguments
    /// * `db` - The pool of connections to the DB
    /// * `http_client` - The HTTP client for scraping from "dilbert.com"
    pub async fn get_latest_date(
        &self,
        db: &Option<RedisPool>,
        http_client: &HttpClient,
    ) -> AppResult<NaiveDate> {
        self.get_data(db, http_client, &()).await
    }

    /// Update the latest date in the cache.
    ///
    /// # Arguments
    /// * `db` - The pool of connections to the DB
    /// * `date` - The date of the latest comic
    pub async fn update_latest_date(
        &self,
        db: &Option<RedisPool>,
        date: &NaiveDate,
    ) -> AppResult<()> {
        self.cache_data(db, date, &()).await
    }
}

#[async_trait(?Send)]
impl Scraper<NaiveDate, ()> for LatestDateScraper {
    /// Get the cached latest date from the database.
    ///
    /// In the rare case that the latest date entry wasn't found in the cache, None is returned.
    /// The boolean return flag indicates whether the cache entry is stale (i.e. it was updated a
    /// long time back) or not.
    async fn get_cached_data(
        &self,
        db: &Option<RedisPool>,
        _reference: &(),
    ) -> AppResult<Option<(NaiveDate, bool)>> {
        let mut conn = if let Some(db) = db {
            db.get().await?
        } else {
            return Ok(None);
        };

        // Heroku enforces an upper limit on the DB memory, and we manually set it to evict the
        // least recently used keys. Thus, since it's possible that this key has been evicted, we
        // use an `Option` for `raw_data`.
        let raw_data: Option<Vec<u8>> = conn.get(LATEST_DATE_KEY).await?;
        debug!("Retrieved raw data from DB for latest date");

        Ok(if let Some(raw_data) = raw_data {
            let info: LatestDateInfo = serde_json::from_slice(raw_data.as_slice())?;
            // The latest date is fresh if it has been updated within the last
            // `LATEST_DATE_REFRESH` hours.
            let last_fresh_time = curr_datetime() - Duration::hours(LATEST_DATE_REFRESH);
            Some((info.date, info.last_check >= last_fresh_time))
        } else {
            None
        })
    }

    /// Cache the latest date into the database.
    async fn cache_data(
        &self,
        db: &Option<RedisPool>,
        date: &NaiveDate,
        _reference: &(),
    ) -> AppResult<()> {
        let mut conn = if let Some(db) = db {
            db.get().await?
        } else {
            return Ok(());
        };

        let new_info = LatestDateInfo {
            date: date.to_owned(),
            last_check: curr_datetime(),
        };
        conn.set(LATEST_DATE_KEY, serde_json::to_vec(&new_info)?)
            .await?;

        info!("Successfully updated latest date in cache to: {}", date);
        Ok(())
    }

    /// Scrape the date of the latest comic from "dilbert.com".
    async fn scrape_data(&self, http_client: &HttpClient, _reference: &()) -> AppResult<NaiveDate> {
        // If there is no comic for this date yet, "dilbert.com" will auto-redirect to the
        // homepage.
        let today = curr_date();
        let url = format!("{}{}", SRC_PREFIX, curr_date().format(SRC_DATE_FMT));

        info!("Trying date \"{}\" for latest comic", today);
        let mut resp = http_client.get(url).send().await?;
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
