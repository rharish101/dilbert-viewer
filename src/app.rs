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
use std::path::Path;
use std::rc::Rc;

use actix_web::{http::header::ContentType, HttpResponse};
use askama::Template;
use chrono::{Duration, NaiveDate};
use tracing::{debug, error, info};

use crate::client::HttpClient;
use crate::constants::{
    APP_URL, DISP_DATE_FMT, FIRST_COMIC, REPO_URL, SRC_BASE_URL, SRC_COMIC_PREFIX, SRC_DATE_FMT,
};
use crate::datetime::str_to_date;
use crate::db::RedisPool;
use crate::errors::{AppError, AppResult, MinificationError};
use crate::scrapers::{ComicData, ComicScraper, LatestDateScraper};
use crate::templates::{ComicTemplate, ErrorTemplate, NotFoundTemplate};

pub struct Viewer<T: RedisPool + 'static> {
    /// The scraper for comics given date
    comic_scraper: ComicScraper<T>,
    /// The scraper for the latest date
    latest_date_scraper: LatestDateScraper<T>,
}

impl<T: RedisPool + Clone + 'static> Viewer<T> {
    /// Initialize all necessary stuff for the viewer.
    pub fn new(db: Option<T>, base_url: String) -> Self {
        let http_client = Rc::new(HttpClient::new(base_url));
        Self {
            comic_scraper: ComicScraper::new(db.clone(), http_client.clone()),
            latest_date_scraper: LatestDateScraper::new(db, http_client),
        }
    }

    /// Get the info about the requested comic and the latest date.
    async fn get_comic_info(
        &self,
        date: &NaiveDate,
        show_latest: bool,
    ) -> AppResult<(ComicData, NaiveDate)> {
        // Execute both in parallel, as they are independent of each other.
        let (comic_data_res, latest_comic_res) = futures::join!(
            self.comic_scraper.get_comic_data(date),
            self.latest_date_scraper.get_latest_date()
        );
        let mut latest_comic = latest_comic_res?;

        let comic_data = if let Some(comic_data) = comic_data_res? {
            // The date of the latest comic is often retrieved from the cache. If there is a comic for
            // a date which is newer than the cached value, then there is a new "latest comic".
            if latest_comic < *date {
                info!(
                    "Found comic \"{date}\" newer than known latest date ({latest_comic}); \
                    updating latest date..."
                );
                latest_comic = *date;
                // Cache the new value of the latest comic date
                self.latest_date_scraper.update_latest_date(date).await?;
            };
            comic_data
        } else {
            // The data is None if the input is invalid (i.e. "dilbert.com" has redirected to the
            // homepage).
            if !show_latest {
                return Err(AppError::NotFound(format!("No comic found for {date}")));
            };
            info!(
                "No comic found for {date}, instead displaying the latest comic ({latest_comic})",
            );
            let comic_data = self.comic_scraper.get_comic_data(&latest_comic).await?;
            if let Some(comic_data) = comic_data {
                comic_data
            } else {
                // This means that the "latest date", either from the DB or by scraping,
                // doesn't have a comic. This should NEVER happen.
                return Err(AppError::Internal(
                    "No comic found for the latest date".into(),
                ));
            }
        };

        Ok((comic_data, latest_comic))
    }

    /// Serve the requested comic.
    ///
    /// If an error is raised, then a 500 internal server error response is returned.
    ///
    /// # Arguments
    /// * `date` - The date of the requested comic
    /// * `show_latest` - If there is no comic found for this date, then whether to show the latest
    ///                   comic
    pub async fn serve_comic(&self, date: &NaiveDate, show_latest: bool) -> HttpResponse {
        match self
            .get_comic_info(date, show_latest)
            .await
            // If `show_latest` is true, then it's possible that `date` is later than the latest
            // comic date. Hence, use `min` to correct it.
            .and_then(|info| serve_template(&min(*date, info.1), &info.0, &info.1))
        {
            Ok(response) => response,
            Err(AppError::NotFound(..)) => serve_404(Some(date)),
            Err(err) => serve_500(&err),
        }
    }
}

fn minify_html(mut html: String) -> AppResult<String> {
    let old_len = html.len();
    let result = minify_html::in_place_str(html.as_mut_str(), &minify_html::Cfg::new());

    // The in-place minification returns a slice to the minified part, but leaves the rest of
    // the string as-is. Hence, we get the length of the slice and truncate the string, since
    // we want to return an owned string.
    let new_len = match result {
        Ok(slice) => slice.len(),
        Err(err) => Err(MinificationError::Html(err))?,
    };
    html.truncate(new_len);

    debug!("Minified HTML from {old_len} bytes to {}", html.len());
    Ok(html)
}

/// Serve the rendered HTML given scraped data.
///
/// # Arguments
/// * `date` - The date of the comic
/// * `comic_data` - The scraped comic data
/// * `latest_comic` - The date of the latest comic
fn serve_template(
    date: &NaiveDate,
    comic_data: &ComicData,
    latest_comic: &NaiveDate,
) -> AppResult<HttpResponse> {
    let first_comic = str_to_date(FIRST_COMIC, SRC_DATE_FMT)?;

    // Links to previous and next comics
    let previous_comic = &max(first_comic, *date - Duration::days(1))
        .format(SRC_DATE_FMT)
        .to_string();
    let next_comic = &min(*latest_comic, *date + Duration::days(1))
        .format(SRC_DATE_FMT)
        .to_string();

    let template = ComicTemplate {
        data: comic_data,
        date_disp: &date.format(DISP_DATE_FMT).to_string(),
        date: &date.format(SRC_DATE_FMT).to_string(),
        first_comic: FIRST_COMIC,
        previous_comic,
        next_comic,
        disable_left_nav: *date == first_comic,
        disable_right_nav: date == latest_comic,
        permalink: &format!(
            "{SRC_BASE_URL}/{SRC_COMIC_PREFIX}{}",
            date.format(SRC_DATE_FMT)
        ),
        app_url: APP_URL,
        repo_url: REPO_URL,
    };
    debug!("Rendering comic template: {template:?}");

    Ok(HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(minify_html(template.render()?)?))
}

/// Serve the requested CSS file with minification, without handling errors.
async fn serve_css_raw(path: &Path) -> AppResult<HttpResponse> {
    let css = match tokio::fs::read(path).await {
        Ok(text) => text,
        Err(err) => return Err(AppError::NotFound(err.to_string())),
    };
    let css_str = std::str::from_utf8(&css)?;

    let minified = match minifier::css::minify(css_str) {
        Ok(minified) => minified.to_string(),
        Err(err) => return Err(MinificationError::Css(err.into()).into()),
    };
    debug!(
        "Minified \"{}\" from {} bytes to {}",
        path.display(),
        css_str.len(),
        minified.len()
    );

    Ok(HttpResponse::Ok()
        .content_type("text/css;charset=utf-8")
        .body(minified))
}

/// Serve the requested CSS file with minification.
///
/// If an error is raised, then a 500 internal server error response is returned.
///
/// # Arguments
/// * `path` - The path to the CSS file
pub async fn serve_css(path: &Path) -> HttpResponse {
    match serve_css_raw(path).await {
        Ok(resp) => resp,
        Err(AppError::NotFound(..)) => serve_404(None),
        Err(err) => serve_500(&err),
    }
}

/// Serve a 404 not found response for invalid URLs, without handling errors.
fn serve_404_raw(date: Option<&NaiveDate>) -> AppResult<HttpResponse> {
    let date_str = date.map(|date| date.format(SRC_DATE_FMT).to_string());
    let template = NotFoundTemplate {
        date: date_str.as_deref(),
        repo_url: REPO_URL,
    };
    debug!("Rendering 404 template: {template:?}");
    Ok(HttpResponse::NotFound()
        .content_type(ContentType::html())
        .body(minify_html(template.render()?)?))
}

/// Serve a 404 not found response for invalid URLs.
///
/// If an error is raised, then a 500 internal server error response is returned.
///
/// # Arguments
/// * `date` - The date of the requested comic, if available. This must be a valid date for
///            which a comic doesn't exist.
pub fn serve_404(date: Option<&NaiveDate>) -> HttpResponse {
    match serve_404_raw(date) {
        Ok(response) => response,
        Err(err) => serve_500(&err),
    }
}

/// Serve a 500 internal server error response.
///
/// # Arguments
/// * `err` - The actual internal server error
pub fn serve_500(err: &AppError) -> HttpResponse {
    let error = &format!("{err}");
    let mut response = HttpResponse::InternalServerError();

    let error_template = ErrorTemplate {
        error,
        repo_url: REPO_URL,
    };
    debug!("Rendering 500 template: {error_template:?}");
    match error_template.render() {
        Ok(webpage) => {
            // Minification can crash, so if it fails, just serve the original. Since
            // minification modifies the input, give it a clone.
            let minified = match minify_html(webpage.clone()) {
                Ok(html) => html,
                Err(err) => {
                    error!("HTML minification crashed with error: {err}");
                    webpage
                }
            };
            response.content_type(ContentType::html()).body(minified)
        }
        Err(err) => {
            error!("Couldn't render Error 500 HTML: {err}");
            // An empty Error 500 response is still better than crashing
            response.finish()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs::read_to_string;

    use actix_web::{
        body::MessageBody,
        http::{
            header::{TryIntoHeaderValue, CONTENT_TYPE},
            StatusCode,
        },
    };
    use test_case::test_case;

    use crate::db::mock::MockPool;

    /// Path to the directory where test HTML files are stored
    const HTML_TEST_CASE_PATH: &str = "testdata/html";

    // NOTE: This does *NOT* check if the minified HTML is equivalent, only that it's parsable.
    #[test_case("empty"; "empty HTML")]
    #[test_case("simple"; "simple HTML")]
    #[test_case("comic"; "comic HTML")]
    #[test_case("minimized"; "already minimized HTML")]
    /// Test whether HTML minification results in a parsable HTML.
    ///
    /// # Arguments
    /// * `file_stem` - The filename stem of the HTML file to be used for testing
    fn test_minified_html_is_parsable(file_stem: &str) {
        let path = format!("{HTML_TEST_CASE_PATH}/{file_stem}.html");
        let html =
            read_to_string(&path).unwrap_or_else(|_| panic!("Couldn't read test case {}", &path));

        let result = minify_html(html).expect("Error minifying HTML");
        // Only checks if the minified HTML is actually parsable.
        tl::parse(&result, tl::ParserOptions::default()).expect("Cannot parse minified HTML");
    }

    /// Test if an HTTP response is a valid HTML page
    fn test_html_response(resp: HttpResponse) {
        // Check the "Content-Type" header.
        assert_eq!(
            resp.headers().get(CONTENT_TYPE),
            Some(&ContentType::html().try_into_value().unwrap()),
            "Response content type is not HTML"
        );

        // Check if response body is valid UTF-8 and the HTML is parsable.
        let body = resp
            .into_body()
            .try_into_bytes()
            .expect("Could not read response body");
        let body_utf8 = std::str::from_utf8(&body).expect("Response body not UTF-8");
        tl::parse(body_utf8, tl::ParserOptions::default()).expect("Response body not valid HTML");
    }

    #[test_case(2000, 1, 1, 2000, 1, 2, "Test"; "past comic")]
    #[test_case(2000, 1, 1, 2000, 1, 1, "Test"; "latest comic")]
    #[test_case(2000, 1, 1, 2000, 1, 2, ""; "empty title")]
    /// Test rendering of comic page templates.
    ///
    /// # Arguments
    /// * `comic_year` - The year of the comic
    /// * `comic_month` - The month of the comic
    /// * `comic_day` - The day of the comic
    /// * `latest_year` - The year of the latest comic
    /// * `latest_month` - The month of the latest comic
    /// * `latest_day` - The day of the latest comic
    /// * `title` - The title of the comic
    fn test_template_rendering(
        comic_year: i32,
        comic_month: u32,
        comic_day: u32,
        latest_year: i32,
        latest_month: u32,
        latest_day: u32,
        title: &str,
    ) {
        let comic_date = NaiveDate::from_ymd_opt(comic_year, comic_month, comic_day)
            .expect("Invalid test parameters");
        let latest_date = NaiveDate::from_ymd_opt(latest_year, latest_month, latest_day)
            .expect("Invalid test parameters");
        let comic_data = ComicData {
            title: title.into(),
            img_url: REPO_URL.into(), // Any URL should technically work.
            img_width: 1,
            img_height: 1,
        };
        let resp = serve_template(&comic_date, &comic_data, &latest_date)
            .expect("Error generating comic page");

        assert_eq!(resp.status(), StatusCode::OK, "Response is not status OK");
        test_html_response(resp);
    }

    #[test_case(Some((2000, 1, 1)); "missing comic")]
    #[test_case(None; "generic 404")]
    /// Test rendering of the 404 not found page template.
    ///
    /// # Arguments
    /// * `date_ymd` - A tuple containing the year, month and day of the missing comic, if any
    fn test_404_page(date_ymd: Option<(i32, u32, u32)>) {
        let date = date_ymd.map(|ymd| {
            NaiveDate::from_ymd_opt(ymd.0, ymd.1, ymd.2).expect("Invalid test parameters")
        });
        let resp = serve_404_raw(date.as_ref()).expect("Error generating 404 page");

        assert_eq!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "Response is not status NOT FOUND"
        );
        test_html_response(resp);
    }

    #[test_case(""; "empty error msg")]
    #[test_case("Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor
    incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation
    ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit
    in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat
    cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.";
    "long error msg")]
    /// Test rendering of the 500 internal server error page template.
    ///
    /// # Arguments
    /// * `error_msg` - The error message to be displayed in the page
    fn test_500_page(error_msg: &str) {
        let resp = serve_500(&AppError::Internal(error_msg.into()));
        assert_eq!(
            resp.status(),
            StatusCode::INTERNAL_SERVER_ERROR,
            "Response is not status INTERNAL SERVER ERROR"
        );
        test_html_response(resp);
    }

    #[test_case("static/styles.css", true; "app CSS")]
    #[test_case("styles.css", false; "missing file")]
    #[test_case("/", false; "invalid CSS path")]
    #[actix_web::test]
    /// Test serving of CSS files.
    ///
    /// # Arguments
    /// * `path` - The path to the CSS file to be used for testing
    /// * `should_serve` - Whether the expected behaviour is to serve a response or to crash
    async fn test_css_serving(path: &str, should_serve: bool) {
        let path = Path::new(path);
        let resp = match serve_css_raw(path).await {
            Ok(resp) => resp,
            Err(AppError::NotFound(err)) => {
                if should_serve {
                    panic!("Error serving CSS that exists: {err}");
                } else {
                    return;
                }
            }
            Err(err) => panic!("Error serving CSS: {err}"),
        };

        // Ensure that no CSS is served when it shouldn't.
        if !should_serve {
            panic!("CSS served even when path doesn't exist");
        }

        // Check the response status.
        assert_eq!(resp.status(), StatusCode::OK, "Response is not status OK");

        // Check the "Content-Type" header.
        let content_type = resp
            .headers()
            .get(CONTENT_TYPE)
            .expect("Missing Content-Type header")
            .to_str()
            .expect("Content-Type header value not valid UTF-8");
        assert!(
            content_type.contains("text/css"),
            "Response content type is not CSS"
        );

        // Check if response body is valid UTF-8 and the CSS is parsable.
        let body = resp
            .into_body()
            .try_into_bytes()
            .expect("Could not read response body");
        let body_utf8 = std::str::from_utf8(&body).expect("Response body not UTF-8");
        // NOTE: This doesn't guarantee that the CSS is valid.
        minifier::css::minify(body_utf8).expect("Response body not valid CSS");
    }

    /// Enum for the state of `Viewer::get_comic_info`.
    enum GetComicInfoState {
        /// Comic info and latest date are valid.
        Normal,
        /// Comic info is valid, but latest date is older than comic date.
        StaleLatestDate,
        /// Comic info is missing, but redirects to latest date as fallback.
        RedirectToLatest,
        /// Comic info is missing, and no redirection is to be done.
        MissingComic,
        /// Crashes with a miscellaneous error.
        Fail,
    }

    /// Get a `Viewer` whose scrapers have been mocked, along with the data it works with.
    ///
    /// # Arguments
    /// * `state` - The state denoting the behaviour of the viewer's scrapers
    ///
    /// # Returns
    /// * The "mocked" viewer
    /// * The test comic date
    /// * The test comic data
    /// * The test latest date
    fn get_mock_viewer(
        state: GetComicInfoState,
    ) -> (Viewer<MockPool>, NaiveDate, ComicData, NaiveDate) {
        let comic_date = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
        let comic_data = ComicData {
            title: String::new(),
            img_url: String::new(),
            img_width: 0,
            img_height: 0,
        };

        // Set up the expected and the cached latest dates.
        let (expected_latest_date, cached_latest_date) =
            if let GetComicInfoState::StaleLatestDate = state {
                // The viewer cannot know of any other latest date if the one it knows about is older
                // than the comic date, so the latest it knows of is the comic date.
                (comic_date, comic_date - Duration::days(1))
            } else {
                let date = if let GetComicInfoState::RedirectToLatest = state {
                    // Redirection to latest only makes sense when the requested comic is newer
                    // than the latest date.
                    comic_date - Duration::days(1)
                } else {
                    NaiveDate::from_ymd_opt(2020, 1, 1).unwrap()
                };
                (date, date)
            };

        // Set up the mock comic scraper.
        let mut mock_comic_scraper = ComicScraper::<MockPool>::default();
        let expected_comic_data = Some(comic_data.clone());
        let comic_exists = matches!(
            state,
            GetComicInfoState::Normal | GetComicInfoState::StaleLatestDate
        );
        mock_comic_scraper
            .expect_get_comic_data()
            // This should only be called once, unless the viewer fails to find the requested comic
            // and the latest date is requested as a fallback.
            .times(if let GetComicInfoState::RedirectToLatest = state {
                2
            } else {
                1
            })
            .returning(move |date| {
                if (date == &comic_date && comic_exists) || date == &expected_latest_date {
                    Ok(expected_comic_data.clone())
                } else {
                    Ok(None)
                }
            });

        // Set up the mock latest date scraper.
        let mut mock_latest_date_scraper = LatestDateScraper::<MockPool>::default();
        mock_latest_date_scraper
            .expect_get_latest_date()
            .return_once(move || {
                // This should be enough to crash the entire function.
                if let GetComicInfoState::Fail = state {
                    Err(AppError::Internal("Manual error".into()))
                } else {
                    Ok(cached_latest_date)
                }
            });
        mock_latest_date_scraper
            .expect_update_latest_date()
            .return_once(|_| Ok(()));

        let viewer = Viewer {
            comic_scraper: mock_comic_scraper,
            latest_date_scraper: mock_latest_date_scraper,
        };
        (viewer, comic_date, comic_data, expected_latest_date)
    }

    #[test_case(GetComicInfoState::Normal; "comic exists, fresh latest date")]
    #[test_case(GetComicInfoState::StaleLatestDate; "comic exists, stale latest date")]
    #[test_case(GetComicInfoState::RedirectToLatest; "missing comic, latest requested")]
    #[test_case(GetComicInfoState::MissingComic; "missing comic, latest not requested")]
    #[actix_web::test]
    /// Test the comic info retrieval by the viewer.
    ///
    /// # Arguments
    /// * `state` - The state denoting the behaviour of the viewer's scrapers
    async fn test_get_comic_info(state: GetComicInfoState) {
        let show_latest = matches!(state, GetComicInfoState::RedirectToLatest);
        let (viewer, comic_date, comic_data, expected_latest_date) = get_mock_viewer(state);

        match viewer.get_comic_info(&comic_date, show_latest).await {
            Ok((result_data, result_latest_date)) => {
                assert_eq!(result_data, comic_data, "Viewer returned wrong comic data");
                assert_eq!(
                    result_latest_date, expected_latest_date,
                    "Viewer returned wrong latest date"
                );
            }
            Err(AppError::NotFound(..)) if !show_latest => {}
            Err(err) => panic!("Viewer failed to get info: {err}"),
        };
    }

    #[test_case(GetComicInfoState::Normal; "comic exists")]
    #[test_case(GetComicInfoState::RedirectToLatest; "missing comic, latest requested")]
    #[test_case(GetComicInfoState::MissingComic; "missing comic, latest not requested")]
    #[test_case(GetComicInfoState::Fail; "crash")]
    #[actix_web::test]
    /// Test the comic info serving.
    ///
    /// # Arguments
    /// * `state` - The state denoting the behaviour of the viewer's scrapers
    async fn test_serve_comic(state: GetComicInfoState) {
        let show_latest = matches!(state, GetComicInfoState::RedirectToLatest);
        let expected_status = match state {
            GetComicInfoState::Normal
            | GetComicInfoState::StaleLatestDate
            | GetComicInfoState::RedirectToLatest => StatusCode::OK,
            GetComicInfoState::MissingComic => StatusCode::NOT_FOUND,
            GetComicInfoState::Fail => StatusCode::INTERNAL_SERVER_ERROR,
        };

        let (viewer, comic_date, _, _) = get_mock_viewer(state);
        let resp = viewer.serve_comic(&comic_date, show_latest).await;
        assert_eq!(resp.status(), expected_status);

        // When the viewer is redirected to the latest date, the HTML content should not use the
        // requested comic date.
        if show_latest {
            let body = resp
                .into_body()
                .try_into_bytes()
                .expect("Could not read response body");
            let html = std::str::from_utf8(&body).expect("Response body not UTF-8");
            assert!(
                !html.contains(&comic_date.format(DISP_DATE_FMT).to_string()),
                "Viewer response uses date of missing comic"
            );
        };
    }
}
