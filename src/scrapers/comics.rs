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
use deadpool_postgres::Pool;
use html_escape::decode_html_entities;
use itertools::Itertools;
use log::{debug, error, info};
use regex::{Error as RegexError, Match, Regex};
use tokio::sync::Mutex;

use crate::constants::{ALT_DATE_FMT, CACHE_LIMIT, DATE_FMT, SRC_PREFIX};
use crate::errors::{AppError, AppResult};
use crate::scrapers::Scraper;
use crate::utils::str_to_date;

// All SQL statements
const UPDATE_LAST_USED_STMT: &str = "UPDATE comic_cache SET last_used = DEFAULT WHERE comic = $1;";
const APPROX_ROWS_STMT: &str = "SELECT reltuples FROM pg_class WHERE relname = 'comic_cache';";
const CLEAN_CACHE_STMT: &str = "
    DELETE FROM comic_cache
    WHERE ctid in
    (SELECT ctid FROM comic_cache ORDER BY last_used LIMIT $1);";
const FETCH_COMIC_STMT: &str =
    "SELECT img_url, title, img_width, img_height FROM comic_cache WHERE comic = $1;";
const INSERT_COMIC_STMT: &str = "
    INSERT INTO comic_cache (comic, img_url, title, img_width, img_height)
    VALUES ($1, $2, $3, $4, $5)
    ON CONFLICT (comic) DO UPDATE
        SET last_used = DEFAULT;";

pub struct ComicData {
    /// The title of the comic
    pub title: String,

    /// The date of that comic as displayed on "dilbert.com"
    // NOTE: The value for the key "dateStr" represents the date in a format which is different
    // from the format used to fetch comics. Also, this date can be different from the given date,
    // as "dilbert.com" can redirect to a different date. This redirection only happens if the
    // input date in invalid.
    pub date_str: String,

    /// The URL to the comic image
    pub img_url: String,

    /// The width of the image
    pub img_width: i32,

    /// The height of the image
    pub img_height: i32,
}

/// Struct for a comic scraper
///
/// This scraper takes a date (in the format used by "dilbert.com") as input.
/// It returns the info about the comic.
pub struct ComicScraper {
    // We want to guard a section of code, not an item, so use `()`.
    insert_comic_lock: Arc<Mutex<()>>,

    // All regexes for scraping
    title_regex: Regex,
    date_str_regex: Regex,
    img_regex: Regex,
}

fn regex_to_app_error(err: RegexError, msg: &str) -> AppError {
    AppError::Regex(err, String::from(msg))
}

impl ComicScraper {
    /// Initialize a comics scraper.
    pub fn new(insert_comic_lock: Arc<Mutex<()>>) -> AppResult<Self> {
        let title_regex = Regex::new("<span class=\"comic-title-name\">([^<]+)</span>")
            .map_err(|err| regex_to_app_error(err, "Invalid regex for comic title"))?;
        let date_str_regex = Regex::new(
            "<date class=\"comic-title-date\" item[pP]rop=\"datePublished\">[^<]*<span>([^<]+)</span>[^<]*<span item[pP]rop=\"copyrightYear\">([^<]+)</span>",
        ).map_err(
            |err| regex_to_app_error(err, "Invalid regex for comic date string"))?;
        let img_regex = Regex::new("<img[^>]*class=\"img-[^>]*width=\"([0-9]+)\"[^>]*height=\"([0-9]+)\"[^>]*src=\"([^\"]+)\"[^>]*>")
            .map_err(|err| regex_to_app_error(err, "Invalid regex for comic image URL"))?;

        Ok(Self {
            insert_comic_lock,
            title_regex,
            date_str_regex,
            img_regex,
        })
    }

    /// Update the last used date for the given comic.
    async fn update_last_used(db_pool: &Option<Pool>, date: NaiveDate) -> AppResult<()> {
        info!("Updating `last_used` for data in cache");
        if let Some(db_pool) = db_pool {
            db_pool
                .get()
                .await?
                .execute(UPDATE_LAST_USED_STMT, &[&date])
                .await?;
        };
        Ok(())
    }

    /// Remove excess rows from the cache.
    async fn clean_cache(db_pool: &Option<Pool>) -> AppResult<()> {
        // This is an approximate of the no. of rows in the `comic_cache` table.  This is much
        // faster than the accurate measurement, as given here:
        // https://wiki.postgresql.org/wiki/Count_estimate
        let db_client = if let Some(db_pool) = db_pool {
            db_pool.get().await?
        } else {
            return Ok(());
        };
        let approx_rows: f32 = db_client
            .query_one(APPROX_ROWS_STMT, &[])
            .await?
            .try_get(0)?;

        if approx_rows < CACHE_LIMIT {
            info!(
                "No. of rows in `comic_cache` ({}) is less than the limit ({})",
                approx_rows, CACHE_LIMIT
            );
            return Ok(());
        }

        let rows_to_clear = approx_rows - CACHE_LIMIT + 1.0;
        info!(
            "No. of rows in `comic_cache` ({}) exceeds the limit ({}); now clearing the oldest {} rows",
            approx_rows, CACHE_LIMIT, rows_to_clear
        );
        db_client
            .execute(CLEAN_CACHE_STMT, &[&rows_to_clear])
            .await?;
        Ok(())
    }

    /// Retrieve the data for the requested comic.
    ///
    /// # Arguments
    /// * `db_pool` - The pool of connections to the DB
    /// * `http_client` - The HTTP client for scraping from the source
    /// * `date` - The date of the requested comic
    pub async fn get_comic_data(
        &self,
        db_pool: &Option<Pool>,
        http_client: &HttpClient,
        date: &str,
    ) -> AppResult<Option<ComicData>> {
        match self.get_data(db_pool, http_client, date).await {
            Ok(comic_data) => Ok(Some(comic_data)),
            Err(AppError::NotFound(_)) => Ok(None),
            Err(err) => Err(err),
        }
    }
}

#[async_trait(?Send)]
impl Scraper<ComicData, ComicData, str> for ComicScraper {
    /// Get the cached comic data from the database.
    ///
    /// If the comic date entry is stale (i.e. it was updated a long time back), or it wasn't
    /// found in the cache, None is returned.
    async fn get_cached_data(
        &self,
        db_pool: &Option<Pool>,
        date: &str,
    ) -> AppResult<Option<ComicData>> {
        let date = str_to_date(date, DATE_FMT)?;
        // The other columns in the table are: `comic`, `last_used`. `comic` is not required here,
        // as we already have the date as a function argument. In case the date given here is
        // invalid (i.e. it would redirect to a comic with a different date), we cannot retrieve
        // the correct date from the cache, as we aren't caching the mapping of incorrect:correct
        // dates. `last_used` will be updated later.
        let rows = if let Some(db_pool) = db_pool {
            db_pool
                .get()
                .await?
                .query(FETCH_COMIC_STMT, &[&date])
                .await?
        } else {
            return Ok(None);
        };

        if rows.is_empty() {
            // This means that the comic for this date wasn't cached, or the date is invalid (i.e.
            // it would redirect to a comic with a different date).
            return Ok(None);
        }

        let comic_row = &rows[0];
        let comic_data = ComicData {
            title: comic_row.try_get(1)?,
            date_str: date.format(ALT_DATE_FMT).to_string(),
            img_url: comic_row.try_get(0)?,
            img_width: comic_row.try_get(2)?,
            img_height: comic_row.try_get(3)?,
        };

        // Update `last_used`, so that this comic isn't accidently de-cached. We want to keep the
        // most recently used comics in the cache, and we are currently using this comic.
        Self::update_last_used(db_pool, date).await?;

        Ok(Some(comic_data))
    }

    /// Cache the comic data into the database.
    async fn cache_data(
        &self,
        db_pool: &Option<Pool>,
        comic_data: &ComicData,
        _date: &str,
    ) -> AppResult<()> {
        let db_client = if let Some(db_pool) = db_pool {
            db_pool.get().await?
        } else {
            return Ok(());
        };

        // The given date can be invalid (i.e. we may have been redirected to a comic with a
        // different date), hence get the correct date from the scraped data.
        let date = str_to_date(&comic_data.date_str, ALT_DATE_FMT)?;

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

        if let Err(err) = Self::clean_cache(db_pool).await {
            // This crash means that there can be some extra rows in the cache. As the row limit is
            // a little conservative, this should not be a big issue.
            error!("Failed to clean comics cache: {:#?}", err);
        }

        if let Err(err) = db_client
            .execute(
                INSERT_COMIC_STMT,
                &[
                    &date,
                    &comic_data.img_url,
                    &comic_data.title,
                    &comic_data.img_width,
                    &comic_data.img_height,
                ],
            )
            .await
        {
            return Err(AppError::from(err));
        } else {
            return Ok(());
        }
    }

    /// Scrape the comic data of the requested date from the source.
    async fn scrape_data(&self, http_client: &HttpClient, date: &str) -> AppResult<ComicData> {
        let url = String::from(SRC_PREFIX) + date;
        let mut resp = http_client.get(url).send().await?;

        if resp.status() == StatusCode::FOUND {
            // Redirected to homepage, implying that there's no comic for this date
            return Err(AppError::NotFound(format!("Comic for {} not found", date)));
        }

        let bytes = resp.body().await?;
        let content = match std::str::from_utf8(&bytes) {
            Ok(text) => text,
            Err(_) => return Err(AppError::Scrape(String::from("Response is not UTF-8"))),
        };

        let title = if let Some(mat) = self
            .title_regex
            .captures(content)
            .and_then(|captures| captures.get(1))
        {
            decode_html_entities(mat.as_str()).into_owned()
        } else {
            // Some comics don't have a title. This is mostly for older comics.
            String::from("")
        };

        let date_str = if let Some(captures) = self
            .date_str_regex
            .captures(content)
            .and_then(|captures| -> Option<Vec<Match>> { captures.iter().collect() })
        {
            captures[1..].iter().map(Match::as_str).join(" ")
        } else {
            return Err(AppError::Scrape(String::from(
                "Error in scraping the date string",
            )));
        };

        let img_info = if let Some(captures) = self.img_regex.captures(content) {
            captures
        } else {
            return Err(AppError::Scrape(String::from(
                "Error in scraping the image's details",
            )));
        };

        let img_width =
            if let Some(width) = img_info.get(1).and_then(|mat| mat.as_str().parse().ok()) {
                width
            } else {
                return Err(AppError::Scrape(String::from(
                    "Error in scraping the image's width",
                )));
            };

        let img_height =
            if let Some(height) = img_info.get(2).and_then(|mat| mat.as_str().parse().ok()) {
                height
            } else {
                return Err(AppError::Scrape(String::from(
                    "Error in scraping the image's height",
                )));
            };

        let img_url = if let Some(mat) = img_info.get(3) {
            String::from(mat.as_str())
        } else {
            return Err(AppError::Scrape(String::from(
                "Error in scraping the image's URL",
            )));
        };

        Ok(ComicData {
            title,
            date_str,
            img_url,
            img_width,
            img_height,
        })
    }
}
