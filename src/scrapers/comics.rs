//! Scraper to get info for requested Dilbert comics
use async_trait::async_trait;
use chrono::NaiveDate;
use deadpool_postgres::Pool;
use html_escape::decode_html_entities;
use itertools::Itertools;
use log::{debug, error, info, warn};
use regex::{Error as RegexError, Match, Regex};
use reqwest::Client as HttpClient;
use tokio::sync::Mutex;
use tokio_postgres::error::SqlState;

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
const FETCH_COMIC_STMT: &str = "SELECT img_url, title FROM comic_cache WHERE comic = $1;";
const INSERT_COMIC_STMT: &str =
    "INSERT INTO comic_cache (comic, img_url, title) VALUES ($1, $2, $3);";

pub(crate) struct ComicData {
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
}

/// Class for a comic scraper
///
/// This scraper takes a date (in the format used by "dilbert.com") as input.
/// It returns the info about the comic.
pub(crate) struct ComicScraper {
    insert_comic_lock: Mutex<()>,

    // All regexes for scraping
    title_regex: Regex,
    date_str_regex: Regex,
    img_url_regex: Regex,
}

fn regex_to_app_error(err: RegexError, msg: &str) -> AppError {
    AppError::Regex(err, String::from(msg))
}

impl ComicScraper {
    /// Initialize a comics scraper
    pub(crate) fn new() -> AppResult<ComicScraper> {
        let title_regex = Regex::new("<span class=\"comic-title-name\">([^<]+)</span>")
            .map_err(|err| regex_to_app_error(err, "Invalid regex for comic title"))?;
        let date_str_regex = Regex::new(
            "<date class=\"comic-title-date\" item[pP]rop=\"datePublished\">[^<]*<span>([^<]+)</span>[^<]*<span item[pP]rop=\"copyrightYear\">([^<]+)</span>",
        ).map_err(
            |err| regex_to_app_error(err, "Invalid regex for comic date string"))?;
        let img_url_regex = Regex::new("<img[^>]*class=\"img-[^>]*src=\"([^\"]+)\"[^>]*>")
            .map_err(|err| regex_to_app_error(err, "Invalid regex for comic image URL"))?;

        Ok(Self {
            // We want to guard a section of code, not an item, so use `()`
            insert_comic_lock: Mutex::new(()),
            title_regex,
            date_str_regex,
            img_url_regex,
        })
    }

    /// Update the last used date for the given comic
    async fn update_last_used(db_pool: &Pool, date: &NaiveDate) -> AppResult<()> {
        info!("Updating `last_used` for data in cache");
        db_pool
            .get()
            .await?
            .execute(UPDATE_LAST_USED_STMT, &[&date])
            .await?;
        Ok(())
    }

    /// Remove excess rows from the cache
    async fn clean_cache(db_pool: &Pool) -> AppResult<()> {
        // This is an approximate of the no. of rows in the `comic_cache` table.  This is much
        // faster than the accurate measurement, as given here:
        // https://wiki.postgresql.org/wiki/Count_estimate
        let db_client = db_pool.get().await?;
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

    /// Retrieve the data for the requested comic
    ///
    /// # Arguments
    /// * `db_pool` - The pool of connections to the DB
    /// * `http_client` - The HTTP client for scraping from the source
    /// * `date` - The date of the requested comic
    pub(crate) async fn get_comic_data(
        &self,
        db_pool: &Pool,
        http_client: &HttpClient,
        date: &str,
    ) -> AppResult<Option<ComicData>> {
        match self.get_data(db_pool, http_client, date).await {
            Ok(data) => Ok(Some(data)),
            Err(AppError::NotFound(_)) => Ok(None),
            Err(err) => Err(err),
        }
    }
}

#[async_trait]
impl Scraper<ComicData, ComicData, str> for ComicScraper {
    /// Get the cached comic data from the database
    ///
    /// If the comic date entry is stale (i.e. it was updated a long time back), or it wasn't
    /// found in the cache, None is returned
    async fn get_cached_data(&self, db_pool: &Pool, date: &str) -> AppResult<Option<ComicData>> {
        let date = str_to_date(date, DATE_FMT)?;
        // The other columns in the table are: `comic`, `last_used`. `comic` is not required here,
        // as we already have the date as a function argument. In case the date given here is
        // invalid (i.e. it would redirect to a comic with a different date), we cannot retrieve
        // the correct date from the cache, as we aren't caching the mapping of incorrect:correct
        // dates. `last_used` will be updated later.
        let rows = db_pool
            .get()
            .await?
            .query(FETCH_COMIC_STMT, &[&date])
            .await?;
        if rows.is_empty() {
            // This means that the comic for this date wasn't cached, or the date is invalid (i.e.
            // it would redirect to a comic with a different date)
            return Ok(None);
        }

        let comic_row = &rows[0];
        let data = ComicData {
            title: comic_row.try_get(1)?,
            date_str: date.format(ALT_DATE_FMT).to_string(),
            img_url: comic_row.try_get(0)?,
        };

        // Update `last_used`, so that this comic isn't accidently de-cached. We want to keep the
        // most recently used comics in the cache, and we are currently using this comic.
        ComicScraper::update_last_used(db_pool, &date).await?;

        Ok(Some(data))
    }

    /// Cache the comic data into the database
    async fn cache_data(&self, db_pool: &Pool, data: &ComicData, _date: &str) -> AppResult<()> {
        // The given date can be invalid (i.e. we may have been redirected to a comic with a
        // different date), hence get the correct date from the scraped data
        let date = str_to_date(&data.date_str, ALT_DATE_FMT)?;

        let db_client = db_pool.get().await?;

        // This lock ensures that the no. of rows in the cache doesn't increase. This can happen,
        // as the code involves first clearing excess rows, then adding a new row. Therefore, the
        // following can increase the no. of rows:
        //   1. Coroutine 1 clears excess rows
        //   2. Coroutine 2 clears no excess rows, as coroutine 1 did them
        //   3. Coroutine 1 adds its row
        //   4. Coroutine 2 adds its row
        debug!("Setting the comic insertion lock");
        let lock_guard = self.insert_comic_lock.lock().await;
        debug!("Got the comic insertion lock");

        if let Err(err) = Self::clean_cache(db_pool).await {
            // This crash means that there can be some extra rows in the cache. As the row limit is
            // a little conservative, this should not be a big issue.
            error!("Failed to clean comics cache: {:#?}", err);
        }

        if let Err(err) = db_client
            .execute(INSERT_COMIC_STMT, &[&date, &data.img_url, &data.title])
            .await
        {
            if let Some(&SqlState::UNIQUE_VIOLATION) = err.code() {
                // This comic date exists, so some other coroutine has already cached this date in
                // parallel. So we can simply update `last_used` later (outside the lock).
                warn!("Trying to cache date {}, which is already cached.", date);
            } else {
                return Err(AppError::from(err));
            }
        } else {
            return Ok(());
        }

        // Release the lock, as it's no longer needed
        debug!("Releasing the comic insertion lock");
        drop(lock_guard);

        // This only executes if caching data led to a UniqueViolation error. The lock isn't needed
        // here, as this command cannot increase the no. of rows in the cache.
        info!("Now trying to update `last_used` in cache.");
        db_client.execute(UPDATE_LAST_USED_STMT, &[&date]).await?;
        Ok(())
    }

    /// Scrape the comic data of the requested date from the source
    async fn scrape_data(&self, http_client: &HttpClient, date: &str) -> AppResult<ComicData> {
        let url = String::from(SRC_PREFIX) + date;
        let resp = http_client.get(url).send().await?;

        if resp.url().path() == "/" {
            // Redirected to homepage, implying that there's no comic for this date
            return Err(AppError::NotFound(format!("Comic for {} not found", date)));
        }

        let content = resp.text().await?;

        let title = if let Some(captures) = self.title_regex.captures(&content) {
            if let Some(mat) = captures.get(1) {
                decode_html_entities(mat.as_str()).into_owned()
            } else {
                // Some comics don't have a title. This is mostly for older comics.
                String::from("")
            }
        } else {
            // Some comics don't have a title. This is mostly for older comics.
            String::from("")
        };

        let date_str = if let Some(captures) = self.date_str_regex.captures(&content) {
            let matches: Option<Vec<Match>> = captures.iter().collect();
            if let Some(captures) = matches {
                captures[1..].iter().map(|mat| mat.as_str()).join(" ")
            } else {
                return Err(AppError::Scrape(String::from(
                    "Error in scraping the date string",
                )));
            }
        } else {
            return Err(AppError::Scrape(String::from(
                "Error in scraping the date string",
            )));
        };

        let img_url = if let Some(captures) = self.img_url_regex.captures(&content) {
            if let Some(mat) = captures.get(1) {
                String::from(mat.as_str())
            } else {
                return Err(AppError::Scrape(String::from(
                    "Error in scraping the image's URL",
                )));
            }
        } else {
            return Err(AppError::Scrape(String::from(
                "Error in scraping the image's URL",
            )));
        };

        Ok(ComicData {
            title,
            date_str,
            img_url,
        })
    }
}
