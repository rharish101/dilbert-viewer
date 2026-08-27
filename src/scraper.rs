// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Scraper to get info for requested Dilbert comics

use futures_util::stream::{StreamExt, iter};
use html_escape::decode_html_entities;
use indicatif::ProgressBar;
use jiff::{Span, civil::Date};
#[cfg(test)]
use mockall::automock;
use reqwest::{Client, StatusCode, redirect::Policy};
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tl::{Bytes, Node, ParserOptions, parse as parse_html};
use tracing::{Level, debug, error, info, instrument, level_enabled, warn};

use crate::constants::{
    FIRST_COMIC, LAST_COMIC, MAX_CONC_SCRAPES, RESP_TIMEOUT, SCRAPE_DELAY, SRC_DATE_FMT, USER_AGENT,
};
use crate::datetime::str_to_date;
use crate::db::{get_comic, insert_comic};
use crate::errors::{ScraperError, ScraperResult};

pub use comic::*;

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct ComicData {
    /// The title of the comic
    pub title: String,

    /// The URL to the comic image
    pub img_url: String,

    /// The width of the image
    pub img_width: i32,

    /// The height of the image
    pub img_height: i32,

    /// The permalink to the comic
    pub permalink: String,
}

mod inner {
    use super::*;

    /// Struct that does the actual scraping/inserting.
    ///
    /// This is separated out for the sole purpose of mock tests.
    pub(super) struct InnerComicScraper {
        pub(super) http_client: Client,
        pub(super) base_url: String,
        pub(super) cdx_url: String,
    }

    /// Convert a response body to a string, erroring if it's not UTF-8.
    fn utf8<'a>(bytes: &'a [u8], what: &str) -> ScraperResult<&'a str> {
        std::str::from_utf8(bytes).map_err(|_| ScraperError::Scrape(format!("{what} is not UTF-8")))
    }

    #[cfg_attr(test, automock)]
    impl InnerComicScraper {
        /// Initialize a comics scraper.
        #[cfg_attr(test, allow(dead_code))]
        pub fn new(base_url: String, cdx_url: String) -> Self {
            let timeout = Duration::from_secs(RESP_TIMEOUT);
            let http_client = Client::builder()
                .timeout(timeout)
                // Don't follow redirects: the comic source signals a missing
                // comic with a 302, which we check for explicitly.
                .redirect(Policy::none())
                .user_agent(USER_AGENT)
                .build()
                .expect("Failed to initialize the HTTP client");
            Self {
                http_client,
                base_url,
                cdx_url,
            }
        }

        /// Scrape the comic data of the requested date from the source.
        pub(super) async fn scrape_data(&self, date: &Date) -> ScraperResult<ComicData> {
            let date_str = date.strftime(SRC_DATE_FMT).to_string();
            let resp = self
                .http_client
                .get(self.cdx_url.replace("{date}", &date_str))
                .send()
                .await?;
            let bytes = resp.bytes().await?;
            debug!("Got CDX API response body of length: {}B", bytes.len());
            let timestamp = utf8(&bytes, "CDX API response")?.trim();
            if timestamp.is_empty() {
                // The CDX API returns an empty body when there's no snapshot.
                return Err(ScraperError::NotFound(format!(
                    "No Wayback Machine snapshot for {date}"
                )));
            }

            let permalink = self
                .base_url
                .replace("{timestamp}", timestamp)
                .replace("{date}", &date_str);
            debug!("CDX API timestamp: {timestamp}, permalink: {permalink}");
            let resp = self.http_client.get(&permalink).send().await?;
            let status = resp.status();
            if status == StatusCode::FOUND {
                // Redirected to homepage, implying that there's no comic for this date
                return Err(ScraperError::NotFound(format!(
                    "Comic for {date} not found"
                )));
            }
            if status != StatusCode::OK {
                error!("Unexpected response status: {status}");
                return Err(ScraperError::Scrape(format!(
                    "Couldn't scrape comic: {:#?}",
                    resp.bytes().await?
                )));
            }

            let bytes = resp.bytes().await?;
            debug!("Got response body of length: {}B", bytes.len());
            let content = utf8(&bytes, "Response")?;

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
                debug!("No title found for comic on: {date}");
                String::new()
            };

            // The image element is the only tag with the class "img-comic"
            let img_attrs =
                if let Some(tag) = get_first_node_by_class("img-comic").and_then(Node::as_tag) {
                    tag.attributes()
                } else {
                    return Err(ScraperError::Scrape(
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

            // The image width and height are the "width" and "height" attributes
            let img_width = get_i32_img_attr("width").ok_or_else(|| {
                ScraperError::Scrape("Error in scraping the image's width".into())
            })?;
            let img_height = get_i32_img_attr("height").ok_or_else(|| {
                ScraperError::Scrape("Error in scraping the image's height".into())
            })?;

            // The image URL is the "src" attribute of the image element
            let img_url = img_attrs
                .get("src")
                .flatten()
                .and_then(Bytes::try_as_utf8_str)
                .map(String::from)
                .ok_or_else(|| ScraperError::Scrape("Error in scraping the image's URL".into()))?;

            let comic_data = ComicData {
                title,
                img_url,
                img_width,
                img_height,
                permalink,
            };
            debug!("Scraped comic data: {comic_data:?}");
            Ok(comic_data)
        }
    }
}

mod comic {
    #[mockall_double::double]
    use super::inner::InnerComicScraper;
    use super::*;

    /// Outcome of a single `scrape_comic_data` call
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(super) enum ScrapeOutcome {
        /// The date was already in the database and wasn't overwritten
        Skipped,
        /// The date had no comic in the source
        Empty,
        /// A comic was scraped and written to the database
        Populated,
    }

    /// Summary of a populate run
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub struct PopulateSummary {
        /// Total number of dates processed
        pub total: u32,
        /// Number of dates already in the database and left alone
        pub skipped: u32,
        /// Number of dates with no comic in the source
        pub empty: u32,
        /// Number of dates scraped and written to the database
        pub populated: u32,
        /// Number of dates that failed
        pub failed: u32,
    }

    /// Struct for a comic scraper
    ///
    /// This scraper takes a date as input and returns the info about the comic.
    pub struct ComicScraper {
        pub(super) inner: InnerComicScraper,
        pub(super) db: DatabaseConnection,
    }

    /// All dates from `FIRST_COMIC` to `LAST_COMIC` (inclusive).
    fn all_dates() -> Vec<Date> {
        let first = str_to_date(FIRST_COMIC, SRC_DATE_FMT)
            .expect("Variable FIRST_COMIC not in format of variable SRC_DATE_FMT");
        let last = str_to_date(LAST_COMIC, SRC_DATE_FMT)
            .expect("Variable LAST_COMIC not in format of variable SRC_DATE_FMT");

        // `Date` doesn't implement `Step`, so iterate manually.
        let mut dates = Vec::new();
        let mut current = first;
        while current <= last {
            dates.push(current);
            current += Span::new().days(1);
        }
        dates
    }

    #[cfg_attr(test, automock)]
    impl ComicScraper {
        /// Initialize a comics scraper.
        #[cfg_attr(test, allow(dead_code))]
        pub fn new(db: DatabaseConnection, base_url: String, cdx_url: String) -> Self {
            Self {
                inner: InnerComicScraper::new(base_url, cdx_url),
                db,
            }
        }

        /// Scrape the comic data for the given date from the source and save it to the database.
        ///
        /// # Arguments
        /// * `date` - The date of the requested comic
        /// * `overwrite` - Whether to overwrite data for existing dates
        #[instrument(skip(self))]
        pub(super) async fn scrape_comic_data(
            &self,
            date: &Date,
            overwrite: bool,
        ) -> ScraperResult<ScrapeOutcome> {
            if !overwrite && get_comic(&self.db, *date).await?.is_some() {
                debug!("Skipping {date}; already in database");
                return Ok(ScrapeOutcome::Skipped);
            }
            match self.inner.scrape_data(date).await {
                Ok(comic_data) => {
                    insert_comic(&self.db, *date, comic_data, overwrite).await?;
                    Ok(ScrapeOutcome::Populated)
                }
                Err(ScraperError::NotFound(msg)) => {
                    debug!("No comic found for {date}: {msg}");
                    Ok(ScrapeOutcome::Empty)
                }
                Err(err) => Err(err),
            }
        }

        /// Scrape comic data for multiple dates in parallel and save it to the database.
        ///
        /// # Arguments
        /// * `scraper` - The scraper to scrape the comics with
        /// * `dates` - The dates to populate
        /// * `overwrite` - Whether to re-scrape and overwrite dates that already exist
        #[instrument(skip(self))]
        pub async fn scrape_comic_data_multi(
            &self,
            dates: Vec<Date>,
            overwrite: bool,
        ) -> PopulateSummary {
            let dates = if dates.is_empty() { all_dates() } else { dates };

            let total = dates.len();
            match dates.as_slice() {
                [] => panic!("No dates found from {FIRST_COMIC} to {LAST_COMIC}"),
                [only_date] => info!("Populating 1 date {only_date} (overwrite: {overwrite})"),
                [first, .., last] => {
                    info!(
                        "Populating {total} dates from {first} to {last} (overwrite: {overwrite})"
                    )
                }
            };

            // Only show the progress bar when tracing is quiet (warn/error),
            // since per-comic logging would clutter it otherwise.
            let progress_bar = if level_enabled!(Level::INFO) || cfg!(test) {
                ProgressBar::hidden()
            } else {
                ProgressBar::new(total as u64)
            };
            tokio_stream::StreamExt::throttle(
                progress_bar.wrap_stream(iter(dates)),
                Duration::from_secs(SCRAPE_DELAY),
            )
            .map(|date| async move { (date, self.scrape_comic_data(&date, overwrite).await) })
            .buffer_unordered(MAX_CONC_SCRAPES)
            .fold(
                PopulateSummary::default(),
                |mut summary, (date, outcome)| async move {
                    match outcome {
                        Ok(ScrapeOutcome::Skipped) => summary.skipped += 1,
                        Ok(ScrapeOutcome::Empty) => summary.empty += 1,
                        Ok(ScrapeOutcome::Populated) => summary.populated += 1,
                        Err(err) => {
                            warn!("Failed to populate {date}: {err}");
                            summary.failed += 1;
                        }
                    }
                    summary.total += 1;
                    summary
                },
            )
            .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::inner::*;
    use super::*;

    use actix_web::http::{Method, StatusCode};
    use test_case::test_case;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    use crate::db::{get_comic, insert_comic, tests::test_db};
    use crate::errors::ScraperError;

    /// Path to the directory where test scraping files are stored
    const SCRAPING_TEST_CASE_PATH: &str = "testdata/scraping";

    /// The outcome of the mocked `InnerComicScraper::scrape_data`.
    #[derive(Clone, Copy)]
    enum MockScrapeOutcome {
        /// Scraping succeeded.
        Ok,
        /// Scraping failed with `AppError::NotFound` (no comic for the date).
        NotFound,
        /// Scraping failed with some other error.
        OtherError,
    }

    #[test_case((2000, 1, 1), false, ("", "https://web.archive.org/web/20150226185430im_/http://assets.amuniversal.com/bdc8a4d06d6401301d80001dd8b71c47", 900, 266); "without title")]
    #[test_case((2020, 1, 1), false, ("Rfp Process", "//web.archive.org/web/20200101060221im_/https://assets.amuniversal.com/7c2789d004020138d860005056a9545d", 900, 280); "with title")]
    #[test_case((2000, 1, 1), true, ("", "", 0, 0); "missing")]
    #[actix_web::test]
    /// Test comic scraping.
    ///
    /// # Arguments
    /// * `date_ymd` - A tuple containing the year, month and day for the comic
    /// * `missing` - Whether the comic is to be indicated as missing
    /// * `comic_data` - The tuple for the comic data containing the title, image URL, image width
    ///                  and image height
    async fn test_comic_scraping(
        date_ymd: (i16, u8, u8),
        missing: bool,
        comic_data: (&str, &str, i32, i32),
    ) {
        let mock_server = MockServer::start().await;
        let date = Date::new(date_ymd.0, date_ymd.1 as i8, date_ymd.2 as i8)
            .expect("Invalid test parameters");

        let scraper = InnerComicScraper::new(
            format!("{}/base/{{timestamp}}/{{date}}", mock_server.uri()),
            format!("{}/cdx/{{date}}", mock_server.uri()),
        );

        let date_str = date.strftime(SRC_DATE_FMT).to_string();
        let timestamp_mock = "2000";
        let expected = ComicData {
            title: comic_data.0.into(),
            img_url: comic_data.1.into(),
            img_width: comic_data.2,
            img_height: comic_data.3,
            permalink: format!("{}/base/{timestamp_mock}/{date_str}", mock_server.uri()),
        };

        let response = if missing {
            // "dilbert.com" uses 302 FOUND to inform that the comic is missing.
            // Response body shouldn't matter, so keep it empty.
            ResponseTemplate::new(StatusCode::FOUND.as_u16())
        } else {
            let html =
                tokio::fs::read_to_string(format!("{SCRAPING_TEST_CASE_PATH}/{date_str}.html"))
                    .await
                    .expect("Couldn't read test page for scraping");
            ResponseTemplate::new(StatusCode::OK.as_u16()).set_body_string(html)
        };

        // Set up the mock server to return the pre-fetched "dilbert.com" response for the given date.
        Mock::given(method(Method::GET.as_str()))
            .and(path(format!("/base/{timestamp_mock}/{date_str}")))
            .respond_with(response)
            .mount(&mock_server)
            .await;

        // Set up the mock server to return a bogus timestamp for the base URL, because this is
        // what the CDX URL is.
        Mock::given(method(Method::GET.as_str()))
            .and(path(format!("/cdx/{date_str}")))
            .respond_with(
                ResponseTemplate::new(StatusCode::OK.as_u16()).set_body_string(timestamp_mock),
            )
            .mount(&mock_server)
            .await;

        // The scraping should fail if and only if the server redirects.
        let scraped = scraper.scrape_data(&date).await;
        if missing {
            let err = scraped.expect_err("Somehow scraped a missing comic");
            assert!(
                matches!(err, ScraperError::NotFound(_)),
                "Expected a NotFound error, got: {err}"
            );
        } else {
            assert_eq!(
                scraped.expect("Failed to scrape comic data"),
                expected,
                "Scraped the wrong comic data"
            );
        }
    }

    #[test_case(false, true, MockScrapeOutcome::Ok; "no overwrite, comic already in database, scrape skipped")]
    #[test_case(false, false, MockScrapeOutcome::Ok; "no overwrite, empty database, scrape succeeds")]
    #[test_case(false, false, MockScrapeOutcome::NotFound; "no overwrite, empty database, scrape not found")]
    #[test_case(false, false, MockScrapeOutcome::OtherError; "no overwrite, empty database, scrape fails")]
    #[test_case(true, true, MockScrapeOutcome::Ok; "overwrite existing comic")]
    #[test_case(true, true, MockScrapeOutcome::NotFound; "overwrite, scrape not found")]
    #[test_case(true, true, MockScrapeOutcome::OtherError; "overwrite, scrape fails")]
    #[test_case(true, false, MockScrapeOutcome::Ok; "overwrite, empty database, scrape succeeds")]
    #[test_case(true, false, MockScrapeOutcome::NotFound; "overwrite, empty database, scrape not found")]
    #[test_case(true, false, MockScrapeOutcome::OtherError; "overwrite, empty database, scrape fails")]
    #[actix_web::test]
    /// Test `scrape_comic_data` across all combinations of overwrite, existing
    /// database state and scrape outcome.
    ///
    /// # Arguments
    /// * `overwrite` - Whether to overwrite data for existing dates
    /// * `existing` - The comic data already present in the database, if any
    /// * `outcome` - The outcome of the mocked scrape
    async fn test_scrape_comic_data(overwrite: bool, existing: bool, outcome: MockScrapeOutcome) {
        let date = Date::new(2000, 1, 1).unwrap();

        let existing_data = ComicData {
            title: "Existing".into(),
            img_url: "https://example.com/existing.png".into(),
            img_width: 580,
            img_height: 140,
            permalink: "https://dilbert.com/strip/2000-01-01".into(),
        };
        let scraped_data = ComicData {
            title: "Scraped".into(),
            img_url: "https://example.com/scraped.png".into(),
            img_width: 580,
            img_height: 140,
            permalink: "https://dilbert.com/strip/2000-01-01".into(),
        };

        // Set up the in-memory database, pre-populating it if the test starts with a comic.
        let db = test_db().await;
        if existing {
            insert_comic(&db, date, existing_data.clone(), false)
                .await
                .expect("Couldn't insert existing comic into DB");
        }

        // Scraping is skipped only if the comic is already in the database and overwrite is off.
        let scrape_calls = if overwrite || !existing { 1 } else { 0 };
        let mut mock_scraper = MockInnerComicScraper::default();
        let scraped_data_clone = scraped_data.clone();
        mock_scraper
            .expect_scrape_data()
            .return_once(move |_| match outcome {
                MockScrapeOutcome::Ok => Ok(scraped_data_clone),
                MockScrapeOutcome::NotFound => Err(ScraperError::NotFound("Missing comic".into())),
                MockScrapeOutcome::OtherError => Err(ScraperError::Scrape("Manual error".into())),
            })
            .times(scrape_calls);

        let scraper = ComicScraper {
            inner: mock_scraper,
            db: db.clone(),
        };

        let expected = match outcome {
            MockScrapeOutcome::OtherError => Err("Scraping error: Manual error".to_string()),
            MockScrapeOutcome::NotFound => Ok(ScrapeOutcome::Empty),
            MockScrapeOutcome::Ok if overwrite || !existing => Ok(ScrapeOutcome::Populated),
            MockScrapeOutcome::Ok => Ok(ScrapeOutcome::Skipped),
        };
        let actual = match scraper.scrape_comic_data(&date, overwrite).await {
            Ok(outcome) => Ok(outcome),
            Err(err) => Err(err.to_string()),
        };
        assert_eq!(
            actual, expected,
            "scrape_comic_data returned the wrong outcome"
        );

        // The scraped data lands in the database only if scraping was attempted and succeeded.
        let expected = match outcome {
            MockScrapeOutcome::Ok if scrape_calls == 1 => Some(scraped_data),
            _ => {
                if existing {
                    Some(existing_data)
                } else {
                    None
                }
            }
        };
        let stored = get_comic(&db, date)
            .await
            .expect("Failed to read comic data from database");
        assert_eq!(
            stored, expected,
            "Scraper left the wrong data in the database"
        );
    }

    /// Test dates, one per outcome.
    fn test_dates() -> (Date, Date, Date, Date) {
        (
            Date::new(2000, 1, 1).unwrap(),
            Date::new(2020, 1, 1).unwrap(),
            Date::new(2010, 1, 4).unwrap(),
            Date::new(2015, 6, 1).unwrap(),
        )
    }

    /// The comic data that the mock scrape "scrapes" for the populated date.
    fn scraped_comic() -> ComicData {
        ComicData {
            title: String::new(),
            img_url: "https://example.com/scraped.png".into(),
            img_width: 900,
            img_height: 266,
            permalink: "https://dilbert.com/strip/2000-01-01".into(),
        }
    }

    /// A scraper with per-date scrape outcomes: the first date succeeds, the
    /// second is never expected to be scraped (it's already in the database),
    /// the third is missing and the fourth fails.
    ///
    /// # Arguments
    /// * `db` - The database connection the scraper uses
    /// * `dates` - The `(populated, skipped, empty, failed)` test dates
    fn outcome_scraper(db: DatabaseConnection, dates: (Date, Date, Date, Date)) -> ComicScraper {
        let (populated_date, _skipped_date, empty_date, failed_date) = dates;

        let mut inner = MockInnerComicScraper::default();
        inner
            .expect_scrape_data()
            .returning(move |date| match *date {
                d if d == populated_date => Ok(scraped_comic()),
                d if d == empty_date => Err(ScraperError::NotFound(format!("No comic for {d}"))),
                d if d == failed_date => Err(ScraperError::Scrape("Scrape failed".into())),
                d => panic!("Scraped the skipped date {d}"),
            });

        ComicScraper { inner, db }
    }

    #[actix_web::test]
    /// Test a run over multiple dates with a mix of outcomes: one date
    /// populates, one is skipped (already in the database, without scraping),
    /// one is missing and one fails. Exercises the buffered stream and all
    /// four outcome branches, and checks the returned summary and the final
    /// database state.
    async fn test_populate_mixed_outcomes() {
        let (populated_date, skipped_date, empty_date, failed_date) = test_dates();

        // Set up the in-memory database, pre-populating the date that should
        // be skipped.
        let db = test_db().await;
        let original = ComicData {
            title: "Original".into(),
            img_url: "https://example.com/original.png".into(),
            img_width: 1,
            img_height: 1,
            permalink: "https://dilbert.com/strip/2020-01-01".into(),
        };
        insert_comic(&db, skipped_date, original.clone(), false)
            .await
            .expect("Couldn't pre-populate test comic");

        let scraper = outcome_scraper(
            db.clone(),
            (populated_date, skipped_date, empty_date, failed_date),
        );
        let summary = scraper
            .scrape_comic_data_multi(
                vec![populated_date, skipped_date, empty_date, failed_date],
                false,
            )
            .await;

        assert_eq!(
            summary,
            PopulateSummary {
                total: 4,
                skipped: 1,
                empty: 1,
                populated: 1,
                failed: 1,
            },
            "Run reported the wrong summary"
        );

        // The scraped comic is stored only for the populated date; the
        // skipped date keeps its original data; the empty and failed dates
        // have no rows.
        assert_eq!(
            get_comic(&db, populated_date).await.unwrap(),
            Some(scraped_comic()),
            "Populated date got the wrong comic"
        );
        assert_eq!(
            get_comic(&db, skipped_date).await.unwrap(),
            Some(original),
            "Skipped date was modified"
        );
        assert_eq!(
            get_comic(&db, empty_date).await.unwrap(),
            None,
            "A row was written for a missing comic"
        );
        assert_eq!(
            get_comic(&db, failed_date).await.unwrap(),
            None,
            "A row was written for a failed date"
        );
    }
}
