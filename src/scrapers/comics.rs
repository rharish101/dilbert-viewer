//! Scraper to get info for requested Dilbert comics
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
use std::sync::Arc;

use async_trait::async_trait;
use awc::{http::StatusCode, Client as HttpClient};
use chrono::NaiveDate;
use html_escape::decode_html_entities;
use log::{debug, error, info};
use sea_orm::{
    sea_query::{Expr, OnConflict},
    ColumnTrait, DatabaseConnection, DbBackend, EntityTrait, FromQueryResult, QueryFilter,
    QueryOrder, QuerySelect, QueryTrait, Set, Statement,
};
use tl::{parse as parse_html, Bytes, Node, ParserOptions};
use tokio::sync::Mutex;

use crate::constants::{CACHE_LIMIT, SRC_DATE_FMT, SRC_PREFIX};
use crate::entities::{comic_cache, prelude::ComicCache};
use crate::errors::{AppError, AppResult};
use crate::scrapers::Scraper;

// Raw SQL statement to get the approximate rows.
// This is an approximate of the no. of rows in the `comic_cache` table. This is much faster than
// the accurate measurement, as given here: https://wiki.postgresql.org/wiki/Count_estimate
const APPROX_ROWS_STMT: &str = "SELECT reltuples FROM pg_class WHERE relname = 'comic_cache';";

/// Type used to capture the result of the approximate row query
#[derive(Debug, FromQueryResult)]
struct ApproxRows {
    reltuples: f32,
}

pub struct ComicData {
    /// The URL to the comic image
    pub img_url: String,

    /// The title of the comic
    pub title: String,

    /// The width of the image
    pub img_width: i32,

    /// The height of the image
    pub img_height: i32,
}

/// Struct for a comic scraper
///
/// This scraper takes a date as input and returns the info about the comic.
pub struct ComicScraper {
    // We want to guard a section of code, not an item, so use `()`.
    insert_comic_lock: Arc<Mutex<()>>,
}

impl ComicScraper {
    /// Initialize a comics scraper.
    pub fn new(insert_comic_lock: Arc<Mutex<()>>) -> Self {
        Self { insert_comic_lock }
    }

    /// Update the last used date for the given comic.
    async fn update_last_used(db: &Option<DatabaseConnection>, date: NaiveDate) -> AppResult<()> {
        info!("Updating `last_used` for data in cache");
        if let Some(db) = db {
            ComicCache::update_many()
                .col_expr(comic_cache::Column::LastUsed, Expr::cust("DEFAULT"))
                .filter(comic_cache::Column::Comic.eq(date))
                .exec(db)
                .await?;
        };
        Ok(())
    }

    /// Remove excess rows from the cache.
    async fn clean_cache(db: &Option<DatabaseConnection>) -> AppResult<()> {
        let db = if let Some(db) = db {
            db
        } else {
            return Ok(());
        };

        // Getting the approximate rows is much faster than getting the actual row count
        let approx_rows = ApproxRows::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Postgres,
            APPROX_ROWS_STMT,
            vec![],
        ))
        .one(db)
        .await?;

        let approx_rows: u64 = if let Some(result) = approx_rows {
            result.reltuples as u64
        } else {
            error!("Couldn't get approximate rows for the comic cache; skipping cleaning");
            return Ok(());
        };

        if approx_rows < CACHE_LIMIT {
            debug!(
                "No. of rows in `comic_cache` ({}) is less than the limit ({})",
                approx_rows, CACHE_LIMIT
            );
            return Ok(());
        }

        let rows_to_clear = approx_rows - CACHE_LIMIT + 1;
        info!(
            "No. of rows in `comic_cache` ({}) exceeds the limit ({}); now clearing the oldest {} rows",
            approx_rows, CACHE_LIMIT, rows_to_clear
        );
        ComicCache::delete_many()
            .filter(
                comic_cache::Column::Comic.in_subquery(
                    ComicCache::find()
                        .order_by_asc(comic_cache::Column::LastUsed)
                        .limit(rows_to_clear)
                        .into_query(),
                ),
            )
            .exec(db)
            .await?;
        Ok(())
    }

    /// Retrieve the data for the requested comic.
    ///
    /// # Arguments
    /// * `db` - The pool of connections to the DB
    /// * `http_client` - The HTTP client for scraping from the source
    /// * `date` - The date of the requested comic
    pub async fn get_comic_data(
        &self,
        db: &Option<DatabaseConnection>,
        http_client: &HttpClient,
        date: &NaiveDate,
    ) -> AppResult<Option<ComicData>> {
        match self.get_data(db, http_client, date).await {
            Ok(comic_data) => Ok(Some(comic_data)),
            Err(AppError::NotFound(_)) => Ok(None),
            Err(err) => Err(err),
        }
    }
}

#[async_trait(?Send)]
impl Scraper<ComicData, NaiveDate> for ComicScraper {
    /// Get the cached comic data from the database.
    ///
    /// If the comic date entry isn't in the cache, None is returned.
    async fn get_cached_data(
        &self,
        db: &Option<DatabaseConnection>,
        date: &NaiveDate,
        _fresh: bool,
    ) -> AppResult<Option<ComicData>> {
        let date = date.to_owned();
        let row = if let Some(db) = db {
            ComicCache::find_by_id(date).one(db).await?
        } else {
            return Ok(None);
        };

        let row = if let Some(row) = row {
            row
        } else {
            // This means that the comic for this date wasn't cached, or the date is invalid (i.e.
            // it would redirect to the homepage).
            return Ok(None);
        };

        // The other columns in the table are: `comic`, `last_used`. `comic` is not required here,
        // as we already have the date as a function argument. In case the date given here is
        // invalid (i.e. it would redirect to the homepage), we cannot retrieve the correct date
        // from the cache, as we aren't caching the mapping of incorrect:correct dates. `last_used`
        // will be updated later.
        let comic_data = ComicData {
            img_url: row.img_url,
            title: row.title,
            img_width: row.img_width,
            img_height: row.img_height,
        };

        // Update `last_used`, so that this comic isn't accidently de-cached. We want to keep the
        // most recently used comics in the cache, and we are currently using this comic.
        Self::update_last_used(db, date).await?;

        Ok(Some(comic_data))
    }

    /// Cache the comic data into the database.
    async fn cache_data(
        &self,
        db: &Option<DatabaseConnection>,
        comic_data: &ComicData,
        date: &NaiveDate,
    ) -> AppResult<()> {
        let db_conn = if let Some(db) = db {
            db
        } else {
            return Ok(());
        };

        // This lock ensures that the no. of rows in the cache doesn't increase. This can happen,
        // as the code involves first clearing excess rows, then adding a new row. Therefore, the
        // following can increase the no. of rows:
        //   1. Coroutine 1 clears excess rows
        //   2. Coroutine 2 clears no excess rows, as coroutine 1 did them
        //   3. Coroutine 1 adds its row
        //   4. Coroutine 2 adds its row
        debug!("Setting the comic insertion lock");
        // This needs to assigned to a variable, otherwise the mutex will immediately unlock
        let _lock_guard = self.insert_comic_lock.lock().await;
        debug!("Got the comic insertion lock");

        if let Err(err) = Self::clean_cache(db).await {
            // This crash means that there can be some extra rows in the cache. As the row limit is
            // a little conservative, this should not be a big issue.
            error!("Failed to clean comics cache: {:#?}", err);
        }

        let row = comic_cache::ActiveModel {
            comic: Set(date.to_owned()),
            img_url: Set(comic_data.img_url.clone()),
            title: Set(comic_data.title.clone()),
            img_width: Set(comic_data.img_width),
            img_height: Set(comic_data.img_height),
            ..Default::default()
        };
        ComicCache::insert(row)
            .on_conflict(
                OnConflict::column(comic_cache::Column::Comic)
                    .value(comic_cache::Column::LastUsed, Expr::cust("DEFAULT"))
                    .clone(),
            )
            .exec(db_conn)
            .await?;
        Ok(())
    }

    /// Scrape the comic data of the requested date from the source.
    async fn scrape_data(
        &self,
        http_client: &HttpClient,
        date: &NaiveDate,
    ) -> AppResult<ComicData> {
        let url = format!("{}{}", SRC_PREFIX, date.format(SRC_DATE_FMT));
        let mut resp = http_client.get(url).send().await?;
        let status = resp.status();

        match status {
            StatusCode::FOUND => {
                // Redirected to homepage, implying that there's no comic for this date
                return Err(AppError::NotFound(format!("Comic for {} not found", date)));
            }
            StatusCode::OK => (),
            _ => {
                error!("Unexpected response status: {}", status);
                return Err(AppError::Scrape(format!(
                    "Couldn't scrape comic: {:#?}",
                    resp.body().await?
                )));
            }
        };

        let bytes = resp.body().await?;
        let content = match std::str::from_utf8(&bytes) {
            Ok(text) => text,
            Err(_) => return Err(AppError::Scrape("Response is not UTF-8".into())),
        };

        let dom = parse_html(content, ParserOptions::default())?;
        let parser = dom.parser();
        let get_first_node_by_class = |class| {
            dom.get_elements_by_class_name(class)
                .next()
                .and_then(|handle| handle.get(parser))
        };

        // The title element is the only tag with the class "comic-title-name"
        let title = if let Some(node) = get_first_node_by_class("comic-title-name") {
            decode_html_entities(&node.inner_text(parser)).into_owned()
        } else {
            // Some comics don't have a title. This is mostly for older comics.
            String::new()
        };

        // The image element is the only tag with the class "img-comic"
        let img_attrs =
            if let Some(tag) = get_first_node_by_class("img-comic").and_then(Node::as_tag) {
                tag.attributes()
            } else {
                return Err(AppError::Scrape(
                    "Error in scraping the image's details".into(),
                ));
            };
        let get_i32_img_attr = |attr| -> Option<i32> {
            img_attrs
                .get(attr)
                .flatten()
                .and_then(Bytes::try_as_utf8_str)
                .and_then(|attr_str| attr_str.parse().ok())
        };

        // The image width is the "width" attribute of the image element
        let img_width = if let Some(width) = get_i32_img_attr("width") {
            width
        } else {
            return Err(AppError::Scrape(
                "Error in scraping the image's width".into(),
            ));
        };

        // The image height is the "height" attribute of the image element
        let img_height = if let Some(height) = get_i32_img_attr("height") {
            height
        } else {
            return Err(AppError::Scrape(
                "Error in scraping the image's height".into(),
            ));
        };

        // The image URL is the "src" attribute of the image element
        let img_url = if let Some(url) = img_attrs
            .get("src")
            .flatten()
            .and_then(Bytes::try_as_utf8_str)
        {
            String::from(url)
        } else {
            return Err(AppError::Scrape("Error in scraping the image's URL".into()));
        };

        Ok(ComicData {
            img_url,
            title,
            img_width,
            img_height,
        })
    }
}
