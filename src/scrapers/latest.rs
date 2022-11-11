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
use chrono::Duration;
use log::{info, warn};
use sea_orm::{sea_query::Expr, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};

use crate::constants::{LATEST_DATE_REFRESH, SRC_DATE_FMT, SRC_PREFIX};
use crate::entities::latest_date;
use crate::entities::prelude::LatestDate;
use crate::errors::{AppError, AppResult};
use crate::scrapers::Scraper;
use crate::utils::{curr_date, curr_datetime, str_to_date};

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
    /// * `db` - The pool of connections to the DB
    /// * `http_client` - The HTTP client for scraping from "dilbert.com"
    pub async fn get_latest_date(
        &self,
        db: &Option<DatabaseConnection>,
        http_client: &HttpClient,
    ) -> AppResult<String> {
        self.get_data(db, http_client, &()).await
    }

    /// Update the latest date in the cache.
    ///
    /// # Arguments
    /// * `db` - The pool of connections to the DB
    /// * `date` - The date of the latest comic
    pub async fn update_latest_date(
        &self,
        db: &Option<DatabaseConnection>,
        date: &str,
    ) -> AppResult<()> {
        self.cache_data(db, date, &()).await
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
        db: &Option<DatabaseConnection>,
        _reference: &(),
    ) -> AppResult<Option<String>> {
        let last_fresh_time = curr_datetime() - Duration::hours(LATEST_DATE_REFRESH);
        let row = if let Some(db) = db {
            LatestDate::find()
                .filter(latest_date::Column::LastCheck.gte(last_fresh_time))
                .one(db)
                .await?
        } else {
            return Ok(None);
        };

        if let Some(row) = row {
            Ok(Some(row.latest.format(SRC_DATE_FMT).to_string()))
        } else {
            Ok(None)
        }
    }

    /// Cache the latest date into the database.
    async fn cache_data(
        &self,
        db: &Option<DatabaseConnection>,
        date: &str,
        _reference: &(),
    ) -> AppResult<()> {
        let db = if let Some(db) = db {
            db
        } else {
            return Ok(());
        };

        let date_obj = str_to_date(date, SRC_DATE_FMT)?;

        // The WHERE condition is not required as there is always only one row in the `latest_date` table.
        // Hence, simply use `update_many`.
        let update_result = LatestDate::update_many()
            .col_expr(latest_date::Column::Latest, Expr::value(date_obj))
            .col_expr(latest_date::Column::LastCheck, Expr::cust("DEFAULT"))
            .exec(db)
            .await?;

        match update_result.rows_affected.cmp(&1) {
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
        let row = latest_date::ActiveModel {
            latest: Set(date_obj),
            ..Default::default()
        };
        LatestDate::insert(row).exec(db).await?;
        Ok(())
    }

    /// Scrape the date of the latest comic from "dilbert.com".
    async fn scrape_data(&self, http_client: &HttpClient, _reference: &()) -> AppResult<String> {
        // If there is no comic for this date yet, "dilbert.com" will auto-redirect to the
        // homepage.
        let latest = curr_date().format(SRC_DATE_FMT).to_string();
        let url = String::from(SRC_PREFIX) + &latest;

        info!("Trying date \"{}\" for latest comic", latest);
        let resp = http_client.get(url).send().await?;

        if resp.status() == StatusCode::FOUND {
            // Redirected to homepage, implying that there's no comic for this date. There must
            // be a comic for the previous date, so use that.
            let date = (curr_date() - Duration::days(1))
                .format(SRC_DATE_FMT)
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
