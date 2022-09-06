//! The viewer app struct and its methods
use std::cmp::{max, min};
use std::time::Duration as TimeDuration;

use actix_web::{http::header::ContentType, HttpResponse};
use askama::Template;
use awc::Client as HttpClient;
use chrono::{Duration as DateDuration, NaiveDate};
use deadpool_postgres::Pool;
use log::info;

use crate::constants::{ALT_DATE_FMT, DATE_FMT, FETCH_TIMEOUT, FIRST_COMIC, REPO, SRC_PREFIX};
use crate::errors::{AppError, AppResult};
use crate::scrapers::{ComicData, ComicScraper, LatestDateScraper};
use crate::templates::{ComicTemplate, ErrorTemplate, NotFoundTemplate};
use crate::utils::str_to_date;

pub struct Viewer {
    /// The pool of connections to the database
    db_pool: Option<Pool>,
    /// The HTTP client for connecting to the server
    http_client: HttpClient,

    /// The scraper for comics given date
    comic_scraper: ComicScraper,
    /// The scraper for the latest date
    latest_date_scraper: LatestDateScraper,
}

/// Initialize the client session for scraping comics.
fn get_http_client() -> HttpClient {
    let timeout = TimeDuration::from_secs(FETCH_TIMEOUT);
    HttpClient::builder().timeout(timeout).finish()
}

impl Viewer {
    /// Initialize all necessary stuff for the viewer.
    pub fn new(db_pool: Option<Pool>) -> AppResult<Self> {
        Ok(Self {
            db_pool,
            http_client: get_http_client(),
            comic_scraper: ComicScraper::new()?,
            latest_date_scraper: LatestDateScraper::new(),
        })
    }

    /// Serve the rendered HTML given scraped data.
    ///
    /// Both input dates must be in the format used by "dilbert.com".
    ///
    /// # Arguments
    /// * `date` - The (possibly corrected) date of the comic
    /// * `data` - The scraped comic data
    /// * `latest_comic` - The date of the latest comic
    async fn serve_template(
        &self,
        date: NaiveDate,
        data: &ComicData,
        latest_comic: NaiveDate,
    ) -> AppResult<HttpResponse> {
        let first_comic = str_to_date(FIRST_COMIC, DATE_FMT)?;

        // Links to previous and next comics
        let previous_comic = &max(first_comic, date - DateDuration::days(1))
            .format(DATE_FMT)
            .to_string();
        let next_comic = &min(latest_comic, date + DateDuration::days(1))
            .format(DATE_FMT)
            .to_string();

        // Whether to disable left/right navigation buttons
        let disable_left_nav = date == first_comic;
        let disable_right_nav = date == latest_comic;

        // Link to original strip on "dilbert.com"
        let permalink = &format!("{}{}", SRC_PREFIX, date.format(DATE_FMT));

        let webpage = ComicTemplate {
            data,
            date: &date.format(DATE_FMT).to_string(),
            first_comic: &first_comic.format(DATE_FMT).to_string(),
            previous_comic,
            next_comic,
            disable_left_nav,
            disable_right_nav,
            permalink,
            repo: REPO,
        }
        .render()?;
        Ok(HttpResponse::Ok()
            .content_type(ContentType::html())
            .body(webpage))
    }

    /// Serve the requested comic, without handling errors.
    async fn serve_comic_raw(&self, date: &str, show_latest: bool) -> AppResult<HttpResponse> {
        // Execute both in parallel, as they are independent of each other.
        let (comic_data_res, latest_comic_res) = futures::join!(
            self.comic_scraper
                .get_comic_data(&self.db_pool, &self.http_client, date),
            self.latest_date_scraper
                .get_latest_date(&self.db_pool, &self.http_client)
        );
        let latest_comic = &latest_comic_res?;

        let comic_data = if let Some(data) = comic_data_res? {
            data
        } else {
            // The data is None if the input is invalid (i.e. "dilbert.com" has redirected to the
            // homepage).
            if show_latest {
                info!(
                    "No comic found for {date}, instead displaying the latest comic ({})",
                    latest_comic
                );
                let data = self
                    .comic_scraper
                    .get_comic_data(&self.db_pool, &self.http_client, latest_comic)
                    .await?;
                if let Some(data) = data {
                    data
                } else {
                    // This means that the "latest date", either from the DB or by scraping,
                    // doesn't have a comic. This should NEVER happen.
                    return Err(AppError::Internal(String::from(
                        "No comic found for the latest date",
                    )));
                }
            } else {
                return Self::serve_404_raw(Some(date));
            }
        };

        let date = &comic_data.date_str;
        let mut latest_comic_obj = str_to_date(latest_comic, DATE_FMT)?;
        let date_obj = str_to_date(date, ALT_DATE_FMT)?;

        // The date of the latest comic is often retrieved from the cache. If there is a comic for
        // a date which is newer than the cached value, then there is a new "latest comic".
        let to_update = if latest_comic_obj < date_obj {
            latest_comic_obj = date_obj;
            true
        } else {
            false
        };

        // This will be awaited along with another future for caching data (if required). They will
        // both be independent of each other, and thus can be run in parallel.
        let template_future = self.serve_template(date_obj, &comic_data, latest_comic_obj);

        if to_update {
            // Cache the new value of the latest comic date
            let update_latest_future = self
                .latest_date_scraper
                .update_latest_date(&self.db_pool, date);
            futures::join!(template_future, update_latest_future).0
        } else {
            template_future.await
        }
    }

    /// Serve the requested comic.
    ///
    /// If an error is raised, then a 500 internal server error response is returned.
    ///
    /// # Arguments
    /// * `date` - The date of the requested comic, in the format used by "dilbert.com"
    /// * `show_latest` - If there is no comic found for this date, then whether to show the latest
    ///                   comic
    pub async fn serve_comic(&self, date: &str, show_latest: bool) -> HttpResponse {
        match self.serve_comic_raw(date, show_latest).await {
            Ok(response) => response,
            Err(err) => Self::serve_500(err),
        }
    }

    /// Serve a 404 not found response for invalid URLs, without handling errors.
    fn serve_404_raw(date: Option<&str>) -> AppResult<HttpResponse> {
        let webpage = NotFoundTemplate { date, repo: REPO }.render()?;
        Ok(HttpResponse::NotFound()
            .content_type(ContentType::html())
            .body(webpage))
    }

    /// Serve a 404 not found response for invalid URLs.
    ///
    /// If an error is raised, then a 500 internal server error response is returned.
    ///
    /// # Arguments
    /// * `date` - The date of the requested comic, if available. This must be a valid date for
    ///            which a comic doesn't exist.
    pub fn serve_404(date: Option<&str>) -> HttpResponse {
        match Self::serve_404_raw(date) {
            Ok(response) => response,
            Err(err) => Self::serve_500(err),
        }
    }

    /// Serve a 500 internal server error response.
    ///
    /// # Arguments
    /// * `err` - The actual internal server error
    pub fn serve_500(err: AppError) -> HttpResponse {
        let error = &format!("{}", err);
        let webpage = ErrorTemplate { error, repo: REPO }.render().unwrap();
        HttpResponse::InternalServerError()
            .content_type(ContentType::html())
            .body(webpage)
    }
}
