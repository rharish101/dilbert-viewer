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
use log::{error, info, warn};
use sea_orm::{sea_query::Expr, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};

use crate::constants::{LATEST_DATE_REFRESH, SRC_DATE_FMT, SRC_PREFIX};
use crate::entities::latest_date;
use crate::entities::prelude::LatestDate;
use crate::errors::{AppError, AppResult};
use crate::scrapers::Scraper;
use crate::utils::{curr_date, curr_datetime};

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
        db: &Option<DatabaseConnection>,
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
        db: &Option<DatabaseConnection>,
        date: &NaiveDate,
    ) -> AppResult<()> {
        self.cache_data(db, date, &()).await
    }
}

#[async_trait(?Send)]
impl Scraper<NaiveDate, ()> for LatestDateScraper {
    /// Get the cached latest date from the database.
    ///
    /// If the latest date entry is stale (i.e. it was updated a long time back) and a fresh entry
    /// is requested, or it wasn't found in the cache, None is returned.
    async fn get_cached_data(
        &self,
        db: &Option<DatabaseConnection>,
        _reference: &(),
        fresh: bool,
    ) -> AppResult<Option<NaiveDate>> {
        let row = if let Some(db) = db {
            let mut query = LatestDate::find();
            if fresh {
                // The latest date is fresh if it has been updated within the last
                // `LATEST_DATE_REFRESH` hours.
                let last_fresh_time = curr_datetime() - Duration::hours(LATEST_DATE_REFRESH);
                query = query.filter(latest_date::Column::LastCheck.gte(last_fresh_time));
            };
            query.one(db).await?
        } else {
            return Ok(None);
        };

        if let Some(row) = row {
            Ok(Some(row.latest))
        } else {
            Ok(None)
        }
    }

    /// Cache the latest date into the database.
    async fn cache_data(
        &self,
        db: &Option<DatabaseConnection>,
        date: &NaiveDate,
        _reference: &(),
    ) -> AppResult<()> {
        let date = date.to_owned();
        let db = if let Some(db) = db {
            db
        } else {
            return Ok(());
        };

        // The WHERE condition is not required as there is always only one row in the `latest_date` table.
        // Hence, simply use `update_many`.
        let update_result = LatestDate::update_many()
            .col_expr(latest_date::Column::Latest, Expr::value(date))
            .col_expr(latest_date::Column::LastCheck, Expr::cust("DEFAULT"))
            .exec(db)
            .await?;

        match update_result.rows_affected.cmp(&1) {
            Ordering::Greater => {
                return Err(AppError::Internal(
                    "The \"latest_date\" table has more than one row, i.e. this table is corrupt"
                        .into(),
                ));
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
            latest: Set(date),
            ..Default::default()
        };
        LatestDate::insert(row).exec(db).await?;
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
