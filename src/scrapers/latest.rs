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
use std::cmp::Ordering;

use async_trait::async_trait;
use awc::{http::StatusCode, Client as HttpClient};
use chrono::{Duration, NaiveDate};
use deadpool_postgres::Pool;
use log::{info, warn};
use tokio_postgres::types::ToSql;

use crate::constants::{DATE_FMT, LATEST_DATE_REFRESH, SRC_PREFIX};
use crate::errors::{AppError, AppResult};
use crate::scrapers::Scraper;
use crate::utils::{curr_date, str_to_date};

// All SQL statements
const LATEST_DATE_STMT: &str = "
    SELECT latest FROM latest_date
    WHERE last_check >= CURRENT_TIMESTAMP - INTERVAL '1 hour' * $1;";
const INSERT_DATE_STMT: &str = "INSERT INTO latest_date (latest) VALUES ($1);";
// The WHERE condition is not required as there is always only one row in the `latest_date` table.
const UPDATE_DATE_STMT: &str = "UPDATE latest_date SET latest = $1;";

/// Struct to scrape the date of the latest Dilbert comic.
///
/// This scraper returns that date in the format used by "dilbert.com".
pub struct LatestDateScraper {}

impl LatestDateScraper {
    /// Initialize a latest date scraper.
    pub fn new() -> Self {
        Self {}
    }

    /// Retrieve the date of the latest comic.
    ///
    /// # Arguments
    /// * `db_pool` - The pool of connections to the DB
    /// * `http_client` - The HTTP client for scraping from "dilbert.com"
    pub async fn get_latest_date(
        &self,
        db_pool: &Option<Pool>,
        http_client: &HttpClient,
    ) -> AppResult<String> {
        self.get_data(db_pool, http_client, &()).await
    }

    /// Update the latest date in the cache.
    ///
    /// # Arguments
    /// * `db_pool` - The pool of connections to the DB
    /// * `date` - The date of the latest comic
    pub async fn update_latest_date(&self, db_pool: &Option<Pool>, date: &str) -> AppResult<()> {
        self.cache_data(db_pool, date, &()).await
    }
}

#[async_trait(?Send)]
impl Scraper<String, str, ()> for LatestDateScraper {
    /// Get the cached latest date from the database.
    ///
    /// If the latest date entry is stale (i.e. it was updated a long time back), or it wasn't
    /// found in the cache, None is returned.
    async fn get_cached_data(
        &self,
        db_pool: &Option<Pool>,
        _reference: &(),
    ) -> AppResult<Option<String>> {
        let rows = if let Some(db_pool) = db_pool {
            db_pool
                .get()
                .await?
                .query(LATEST_DATE_STMT, &[&LATEST_DATE_REFRESH])
                .await?
        } else {
            return Ok(None);
        };

        if rows.is_empty() {
            Ok(None)
        } else {
            Ok(Some(
                rows[0]
                    .try_get::<usize, NaiveDate>(0)?
                    .format(DATE_FMT)
                    .to_string(),
            ))
        }
    }

    /// Cache the latest date into the database.
    async fn cache_data(
        &self,
        db_pool: &Option<Pool>,
        date: &str,
        _reference: &(),
    ) -> AppResult<()> {
        let db_client = if let Some(db_pool) = db_pool {
            db_pool.get().await?
        } else {
            return Ok(());
        };
        let query_params: [&(dyn ToSql + Sync); 1] = [&str_to_date(date, DATE_FMT)?];
        let rows_updated = db_client
            .execute(UPDATE_DATE_STMT, query_params.as_slice())
            .await?;

        match rows_updated.cmp(&1) {
            Ordering::Greater => {
                let msg =
                    "The \"latest_date\" table has more than one row, i.e. this table is corrupt";
                return Err(AppError::Internal(String::from(msg)));
            }
            Ordering::Less => (),
            Ordering::Equal => {
                info!("Successfully updated latest date in cache");
                return Ok(());
            }
        }

        // No rows were updated, so the "latest_date" table must be empty. This should only happen
        // if this table was cleared manually, or this is the first run of this code on this
        // database.
        warn!("Couldn't update latest date in cache, presumably because it was missing. This should only happen on the first run. Trying to insert it now.");
        db_client
            .execute(INSERT_DATE_STMT, query_params.as_slice())
            .await?;
        Ok(())
    }

    /// Scrape the date of the latest comic from "dilbert.com".
    async fn scrape_data(&self, http_client: &HttpClient, _reference: &()) -> AppResult<String> {
        // If there is no comic for this date yet, "dilbert.com" will auto-redirect to the
        // homepage.
        let latest = curr_date().format(DATE_FMT).to_string();
        let url = String::from(SRC_PREFIX) + &latest;

        info!("Trying date \"{}\" for latest comic", latest);
        let resp = http_client.get(url).send().await?;

        if resp.status() == StatusCode::FOUND {
            // Redirected to homepage, implying that there's no comic for this date. There must
            // be a comic for the previous date, so use that.
            let date = (curr_date() - Duration::days(1))
                .format(DATE_FMT)
                .to_string();
            info!(
                "No comic found for today ({}); using date: {}",
                latest, date
            );
            Ok(date)
        } else {
            info!(
                "Found comic for today ({}); using it as latest date",
                latest
            );
            Ok(latest)
        }
    }
}
