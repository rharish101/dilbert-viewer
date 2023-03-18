// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The viewer app struct and its methods
use std::cmp::{max, min};
use std::path::Path;
use std::rc::Rc;

use actix_web::{http::header::ContentType, HttpResponse};
use askama::Template;
use chrono::{Duration, NaiveDate};
use tracing::{debug, error};

use crate::client::HttpClient;
use crate::constants::{
    APP_URL, DISP_DATE_FMT, FIRST_COMIC, LAST_COMIC, REPO_URL, SRC_BASE_URL, SRC_COMIC_PREFIX,
    SRC_DATE_FMT,
};
use crate::datetime::str_to_date;
use crate::db::RedisPool;
use crate::errors::{AppError, AppResult, MinificationError};
use crate::scrapers::{ComicData, ComicScraper};
use crate::templates::{ComicTemplate, ErrorTemplate, NotFoundTemplate};

pub struct Viewer<T: RedisPool + 'static> {
    /// The scraper for comics given date
    comic_scraper: ComicScraper<T>,
}

impl<T: RedisPool + Clone + 'static> Viewer<T> {
    /// Initialize all necessary stuff for the viewer.
    pub fn new(db: Option<T>, base_url: String) -> Self {
        let http_client = Rc::new(HttpClient::new(base_url));
        Self {
            comic_scraper: ComicScraper::new(db, http_client),
        }
    }

    /// Get the info about the requested comic.
    async fn get_comic_info(&self, date: &NaiveDate) -> AppResult<ComicData> {
        if let Some(comic_data) = self.comic_scraper.get_comic_data(date).await? {
            Ok(comic_data)
        } else {
            Err(AppError::NotFound(format!("No comic found for {date}")))
        }
    }

    /// Serve the requested comic.
    ///
    /// If an error is raised, then a 500 internal server error response is returned.
    ///
    /// # Arguments
    /// * `date` - The date of the requested comic
    pub async fn serve_comic(&self, date: &NaiveDate) -> HttpResponse {
        match self
            .get_comic_info(date)
            .await
            .and_then(|info| serve_template(date, &info))
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
fn serve_template(date: &NaiveDate, comic_data: &ComicData) -> AppResult<HttpResponse> {
    let first_comic = str_to_date(FIRST_COMIC, SRC_DATE_FMT)?;
    let last_comic = str_to_date(LAST_COMIC, SRC_DATE_FMT)?;

    // Links to previous and next comics
    let previous_comic = &max(first_comic, *date - Duration::days(1))
        .format(SRC_DATE_FMT)
        .to_string();
    let next_comic = &min(last_comic, *date + Duration::days(1))
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
        disable_right_nav: *date == last_comic,
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

    #[test_case(2000, 1, 1, "Test"; "comic with title")]
    #[test_case(2000, 1, 1, ""; "comic without title")]
    /// Test rendering of comic page templates.
    ///
    /// # Arguments
    /// * `comic_year` - The year of the comic
    /// * `comic_month` - The month of the comic
    /// * `comic_day` - The day of the comic
    /// * `title` - The title of the comic
    fn test_template_rendering(comic_year: i32, comic_month: u32, comic_day: u32, title: &str) {
        let comic_date = NaiveDate::from_ymd_opt(comic_year, comic_month, comic_day)
            .expect("Invalid test parameters");
        let comic_data = ComicData {
            title: title.into(),
            img_url: REPO_URL.into(), // Any URL should technically work.
            img_width: 1,
            img_height: 1,
        };
        let resp = serve_template(&comic_date, &comic_data).expect("Error generating comic page");

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
        let resp = serve_500(&AppError::Scrape(error_msg.into()));
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
    #[derive(PartialEq, Eq)]
    enum GetComicInfoState {
        /// Comic info.
        Found,
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
    fn get_mock_viewer(state: GetComicInfoState) -> (Viewer<MockPool>, NaiveDate, ComicData) {
        let comic_date = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
        let comic_data = ComicData {
            title: String::new(),
            img_url: String::new(),
            img_width: 0,
            img_height: 0,
        };

        // Set up the mock comic scraper.
        let mut mock_comic_scraper = ComicScraper::<MockPool>::default();
        let expected_comic_data = Some(comic_data.clone());
        mock_comic_scraper
            .expect_get_comic_data()
            .times(1)
            .returning(move |date| match state {
                GetComicInfoState::Found if date == &comic_date => Ok(expected_comic_data.clone()),
                GetComicInfoState::Fail => Err(AppError::Scrape("Manual error".into())),
                _ => Ok(None),
            });

        let viewer = Viewer {
            comic_scraper: mock_comic_scraper,
        };
        (viewer, comic_date, comic_data)
    }

    #[test_case(GetComicInfoState::Found; "comic exists")]
    #[test_case(GetComicInfoState::MissingComic; "missing comic")]
    #[actix_web::test]
    /// Test the comic info retrieval by the viewer.
    ///
    /// # Arguments
    /// * `state` - The state denoting the behaviour of the viewer's scrapers
    async fn test_get_comic_info(state: GetComicInfoState) {
        let is_missing = state == GetComicInfoState::MissingComic;
        let (viewer, comic_date, comic_data) = get_mock_viewer(state);
        match viewer.get_comic_info(&comic_date).await {
            Ok(result_data) => {
                assert_eq!(result_data, comic_data, "Viewer returned wrong comic data");
            }
            Err(AppError::NotFound(..)) if is_missing => {}
            Err(err) => panic!("Viewer failed to get info: {err}"),
        };
    }

    #[test_case(GetComicInfoState::Found; "comic exists")]
    #[test_case(GetComicInfoState::MissingComic; "missing comic")]
    #[test_case(GetComicInfoState::Fail; "crash")]
    #[actix_web::test]
    /// Test the comic info serving.
    ///
    /// # Arguments
    /// * `state` - The state denoting the behaviour of the viewer's scrapers
    async fn test_serve_comic(state: GetComicInfoState) {
        let expected_status = match state {
            GetComicInfoState::Found => StatusCode::OK,
            GetComicInfoState::MissingComic => StatusCode::NOT_FOUND,
            GetComicInfoState::Fail => StatusCode::INTERNAL_SERVER_ERROR,
        };

        let (viewer, comic_date, _) = get_mock_viewer(state);
        let resp = viewer.serve_comic(&comic_date).await;
        assert_eq!(resp.status(), expected_status);
    }
}
