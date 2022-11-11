//! The viewer app struct and its methods
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
use std::cmp::{max, min};
use std::sync::Arc;
use std::time::Duration as TimeDuration;

use actix_web::{http::header::ContentType, HttpResponse};
use askama::Template;
use awc::Client as HttpClient;
use chrono::{Duration as DateDuration, NaiveDate};
use deadpool_postgres::Pool;
use log::{debug, error, info};
use tokio::sync::Mutex;

use crate::constants::{DISP_DATE_FMT, FIRST_COMIC, REPO, RESP_TIMEOUT, SRC_DATE_FMT, SRC_PREFIX};
use crate::errors::{AppError, AppResult, MinificationError};
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
    let timeout = TimeDuration::from_secs(RESP_TIMEOUT);
    HttpClient::builder()
        .disable_redirects()
        .timeout(timeout)
        .finish()
}

impl Viewer {
    /// Initialize all necessary stuff for the viewer.
    pub fn new(db_pool: Option<Pool>, insert_comic_lock: Arc<Mutex<()>>) -> Self {
        Self {
            db_pool,
            http_client: get_http_client(),
            comic_scraper: ComicScraper::new(insert_comic_lock),
            latest_date_scraper: LatestDateScraper::new(),
        }
    }

    fn minify_html(mut html: String) -> AppResult<String> {
        let old_len = html.len();
        let result = minify_html::in_place_str(html.as_mut_str(), &minify_html::Cfg::new());
        let new_len = match result {
            Ok(slice) => slice.len(),
            Err(err) => Err(MinificationError::Html(err))?,
        };
        html.truncate(new_len);
        debug!("Minified HTML from {} bytes to {}", old_len, html.len());
        Ok(html)
    }

    /// Serve the rendered HTML given scraped data.
    ///
    /// Both input dates must be in the format used by "dilbert.com".
    ///
    /// # Arguments
    /// * `date` - The (possibly corrected) date of the comic
    /// * `comic_data` - The scraped comic data
    /// * `latest_comic` - The date of the latest comic
    fn serve_template(
        date: NaiveDate,
        comic_data: &ComicData,
        latest_comic: NaiveDate,
    ) -> AppResult<HttpResponse> {
        let first_comic = str_to_date(FIRST_COMIC, SRC_DATE_FMT)?;

        // Links to previous and next comics
        let previous_comic = &max(first_comic, date - DateDuration::days(1))
            .format(SRC_DATE_FMT)
            .to_string();
        let next_comic = &min(latest_comic, date + DateDuration::days(1))
            .format(SRC_DATE_FMT)
            .to_string();

        // Whether to disable left/right navigation buttons
        let disable_left_nav = date == first_comic;
        let disable_right_nav = date == latest_comic;

        // Link to original strip on "dilbert.com"
        let permalink = &format!("{}{}", SRC_PREFIX, date.format(SRC_DATE_FMT));

        let webpage = ComicTemplate {
            data: comic_data,
            date_disp: &comic_data.date.format(DISP_DATE_FMT).to_string(),
            date: &date.format(SRC_DATE_FMT).to_string(),
            first_comic: &first_comic.format(SRC_DATE_FMT).to_string(),
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
            .body(Self::minify_html(webpage)?))
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

        let comic_data = if let Some(comic_data) = comic_data_res? {
            comic_data
        } else {
            // The data is None if the input is invalid (i.e. "dilbert.com" has redirected to the
            // homepage).
            if show_latest {
                info!(
                    "No comic found for {date}, instead displaying the latest comic ({})",
                    latest_comic
                );
                let comic_data = self
                    .comic_scraper
                    .get_comic_data(&self.db_pool, &self.http_client, latest_comic)
                    .await?;
                if let Some(comic_data) = comic_data {
                    comic_data
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

        let date = comic_data.date;
        let mut latest_comic_date = str_to_date(latest_comic, SRC_DATE_FMT)?;

        // The date of the latest comic is often retrieved from the cache. If there is a comic for
        // a date which is newer than the cached value, then there is a new "latest comic".
        if latest_comic_date < date {
            latest_comic_date = date;
            // Cache the new value of the latest comic date
            self.latest_date_scraper
                .update_latest_date(&self.db_pool, &date.format(SRC_DATE_FMT).to_string())
                .await?;
        };

        Self::serve_template(date, &comic_data, latest_comic_date)
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
            Err(err) => Self::serve_500(&err),
        }
    }

    /// Serve the requested CSS file with minification.
    ///
    /// If an error is raised, then a 500 internal server error response is returned.
    ///
    /// # Arguments
    /// * `path` - The path to the CSS file
    pub async fn serve_css(path: &std::path::Path) -> HttpResponse {
        let css = if let Ok(text) = tokio::fs::read(path).await {
            text
        } else {
            return Self::serve_404(None);
        };
        let css_str = match std::str::from_utf8(&css) {
            Ok(css_str) => css_str,
            Err(err) => return Self::serve_500(&AppError::Utf8(err)),
        };

        let minified = match minifier::css::minify(css_str) {
            Ok(minified) => minified.to_string(),
            Err(err) => {
                return Self::serve_500(&AppError::Minify(MinificationError::Css(String::from(
                    err,
                ))))
            }
        };
        debug!(
            "Minified \"{}\" from {} bytes to {}",
            path.display(),
            css_str.len(),
            minified.len()
        );

        HttpResponse::Ok()
            .content_type("text/css;charset=utf-8")
            .body(minified)
    }

    /// Serve a 404 not found response for invalid URLs, without handling errors.
    fn serve_404_raw(date: Option<&str>) -> AppResult<HttpResponse> {
        let webpage = NotFoundTemplate { date, repo: REPO }.render()?;
        Ok(HttpResponse::NotFound()
            .content_type(ContentType::html())
            .body(Self::minify_html(webpage)?))
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
            Err(err) => Self::serve_500(&err),
        }
    }

    /// Serve a 500 internal server error response.
    ///
    /// # Arguments
    /// * `err` - The actual internal server error
    pub fn serve_500(err: &AppError) -> HttpResponse {
        let error = &format!("{}", err);
        let mut response = HttpResponse::InternalServerError();

        let error_template = ErrorTemplate { error, repo: REPO };
        match error_template.render() {
            Ok(webpage) => {
                let minified = if let Ok(html) = Self::minify_html(webpage.clone()) {
                    html
                } else {
                    webpage
                };
                response.content_type(ContentType::html()).body(minified)
            }
            Err(err) => {
                error!("Couldn't render Error 500 HTML: {}", err);
                response.finish()
            }
        }
    }
}
