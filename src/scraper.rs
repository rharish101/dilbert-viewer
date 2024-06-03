// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Scraper to get info for requested Dilbert comics

use awc::{http::StatusCode, Client};
use chrono::NaiveDate;
use html_escape::decode_html_entities;
#[cfg(test)]
use mockall::automock;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tl::{parse as parse_html, Bytes, Node, ParserOptions};
use tracing::{debug, error, info, instrument, warn};

use crate::constants::{RESP_TIMEOUT, SRC_BASE_URL, SRC_COMIC_PREFIX, SRC_DATE_FMT};
use crate::db::{RedisPool, SerdeAsyncCommands};
use crate::errors::{AppError, AppResult};

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

    /// Struct that does the actual scraping/caching.
    ///
    /// This is separated out for the sole purpose of mock tests.
    pub(super) struct InnerComicScraper<T: RedisPool + 'static> {
        pub(super) db: Option<T>,
        pub(super) http_client: Client,
        pub(super) base_url: String,
        pub(super) cdx_url: String,
    }

    #[cfg_attr(test, automock)]
    impl<T: RedisPool + 'static> InnerComicScraper<T> {
        /// Initialize a comics scraper.
        #[cfg_attr(test, allow(dead_code))]
        pub fn new(db: Option<T>, base_url: String, cdx_url: String) -> Self {
            let timeout = Duration::from_secs(RESP_TIMEOUT);
            let http_client = Client::builder().timeout(timeout).finish();
            Self {
                db,
                http_client,
                base_url,
                cdx_url,
            }
        }

        /// Get the cached comic data from the database.
        ///
        /// If the comic date entry isn't in the cache, None is returned.
        pub(super) async fn get_cached_data(
            &self,
            date: &NaiveDate,
        ) -> AppResult<Option<(ComicData, bool)>> {
            let mut conn = if let Some(db) = &self.db {
                db.get().await?
            } else {
                return Ok(None);
            };

            // None would mean that the comic for this date wasn't cached, or the date is invalid (i.e.
            // it would redirect to the homepage).
            let comic_data: Option<ComicData> = conn.get(date).await?;
            debug!("Retrieved data from DB: {comic_data:?}");
            Ok(comic_data.map(|comic_data| (comic_data, true)))
        }

        /// Cache the comic data into the database.
        pub(super) async fn cache_data(
            &self,
            comic_data: &ComicData,
            date: &NaiveDate,
        ) -> AppResult<()> {
            let mut conn = if let Some(db) = &self.db {
                db.get().await?
            } else {
                return Ok(());
            };

            debug!("Attempting to update cache with: {comic_data:?}");
            conn.set(date, comic_data).await?;
            info!("Successfully cached data for {date} in cache");
            Ok(())
        }

        /// Scrape the comic data of the requested date from the source.
        pub(super) async fn scrape_data(&self, date: &NaiveDate) -> AppResult<ComicData> {
            let path = format!("{SRC_COMIC_PREFIX}{}", date.format(SRC_DATE_FMT));
            let mut resp = self
                .http_client
                .get(&self.cdx_url.replace("{}", &format!("{SRC_BASE_URL}{path}")))
                .send()
                .await?;
            let bytes = resp.body().await?;
            debug!("Got CDX API response body of length: {}B", bytes.len());
            let timestamp = match std::str::from_utf8(&bytes) {
                Ok(text) => text.trim(),
                Err(_) => return Err(AppError::Scrape("CDX API response is not UTF-8".into())),
            };

            let permalink = format!("{}/{path}", self.base_url.replace("{}", timestamp));
            let mut resp = self.http_client.get(&permalink).send().await?;
            let status = resp.status();

            match status {
                StatusCode::FOUND => {
                    // Redirected to homepage, implying that there's no comic for this date
                    return Err(AppError::NotFound(format!("Comic for {date} not found")));
                }
                StatusCode::OK => (),
                _ => {
                    error!("Unexpected response status: {status}");
                    return Err(AppError::Scrape(format!(
                        "Couldn't scrape comic: {:#?}",
                        resp.body().await?
                    )));
                }
            };

            let bytes = resp.body().await?;
            debug!("Got response body of length: {}B", bytes.len());
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
                debug!("No title found for comic on: {date}");
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

    /// Struct for a comic scraper
    ///
    /// This scraper takes a date as input and returns the info about the comic.
    pub struct ComicScraper<T: RedisPool + 'static>(pub(super) InnerComicScraper<T>);

    #[cfg_attr(test, automock)]
    impl<T: RedisPool + 'static> ComicScraper<T> {
        /// Initialize a comics scraper.
        #[cfg_attr(test, allow(dead_code))]
        pub fn new(db: Option<T>, base_url: String, cdx_url: String) -> Self {
            Self(InnerComicScraper::new(db, base_url, cdx_url))
        }

        /// Retrieve the data for the requested comic.
        ///
        /// # Arguments
        /// * `date` - The date of the requested comic
        #[instrument(skip(self))]
        pub async fn get_comic_data(&self, date: &NaiveDate) -> AppResult<Option<ComicData>> {
            let stale_data = match self.0.get_cached_data(date).await {
                Ok(Some((comic_data, true))) => {
                    info!("Successful retrieval from cache");
                    return Ok(Some(comic_data));
                }
                Ok(Some((comic_data, false))) => Some(comic_data),
                Ok(None) => None,
                Err(err) => {
                    // Better to re-scrape now than crash unexpectedly, so simply log the error.
                    error!("Error retrieving from cache: {err}");
                    None
                }
            };

            info!("Couldn't fetch fresh data from cache; trying to scrape");
            let err = match self.0.scrape_data(date).await {
                Ok(comic_data) => {
                    info!("Scraped data from source");
                    if let Err(err) = self.0.cache_data(&comic_data, date).await {
                        error!("Error caching data: {err}");
                    }
                    info!("Cached scraped data");
                    return Ok(Some(comic_data));
                }
                Err(err) => err,
            };

            // Scraping failed for some reason, so use the "stale" cache entry, if available.
            error!("Scraping failed with error: {err}");

            match stale_data {
                // No stale cache entry exists, so raise the scraping error.
                None => match err {
                    AppError::NotFound(_) => Ok(None),
                    _ => Err(err),
                },

                // Return the "stale" cache entry
                Some(comic_data) => {
                    warn!("Returning stale cache entry");
                    Ok(Some(comic_data))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::inner::*;
    use super::*;

    use actix_web::http::{Method, StatusCode};
    use redis::{Cmd, Value};
    use redis_test::{IntoRedisValue, MockCmd, MockRedisConnection};
    use test_case::test_case;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    use crate::db::mock::MockPool;
    use crate::errors::AppError;

    /// Path to the directory where test scraping files are stored
    const SCRAPING_TEST_CASE_PATH: &str = "testdata/scraping";

    /// Enum for the state of the mock struct during cache retrieval.
    pub enum GetCacheState {
        /// Retrieve a fresh value.
        Fresh,
        /// Retrieve a stale value.
        Stale,
        /// Value not found in cache.
        NotFound,
        /// Retrieval crashes.
        Fail,
    }

    #[test_case(GetCacheState::Fresh; "comic in cache")]
    #[test_case(GetCacheState::NotFound; "empty cache")]
    #[actix_web::test]
    /// Test cache retrieval of a comic.
    ///
    /// # Arguments
    /// * `status` - Status for the cache retrieval
    async fn test_comic_cache_retrieval(status: GetCacheState) {
        // Set up the expected return values, and the entry to store in the mock cache.
        let date = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
        let comic_data = ComicData {
            title: String::new(),
            img_url: String::new(),
            img_width: 0,
            img_height: 0,
            permalink: String::new(),
        };
        let expected = match status {
            GetCacheState::Fresh => {
                Some((comic_data, true)) // Entry should always be fresh.
            }
            GetCacheState::NotFound => None,
            GetCacheState::Stale | GetCacheState::Fail => panic!("Invalid test parameter"),
        };

        // Set up the mock Redis command that the scraper is expected to request.
        let cache_key = serde_json::to_vec(&date).expect("Couldn't serialize mock cache key");
        let cache_value = if let Some((ref comic_data, _)) = expected {
            serde_json::to_vec(&comic_data)
                .expect("Couldn't serialize mock cache value")
                .into_redis_value()
        } else {
            Value::Nil
        };
        let retrieval_cmd = MockCmd::new(Cmd::get(cache_key), Ok(cache_value));

        // Max pool size is one, since only one connection is needed.
        let db = MockPool::new(1);
        if let Err((_, err)) = db.add(MockRedisConnection::new([retrieval_cmd])).await {
            panic!("Couldn't add mock DB connection to mock DB pool: {err}");
        };

        // The HTTP client shouldn't be used, so make the URLs empty.
        let scraper = InnerComicScraper::new(Some(db), String::new(), String::new());
        let result = scraper
            .get_cached_data(&date)
            .await
            .expect("Failed to get comic data from cache");
        assert_eq!(
            result, expected,
            "Retrieved the wrong comic data from cache"
        );
    }

    #[actix_web::test]
    /// Test cache storage of a comic.
    async fn test_comic_cache_storage() {
        // Set up the entry to store in the mock cache.
        let date = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
        let comic_data = ComicData {
            title: String::new(),
            img_url: String::new(),
            img_width: 0,
            img_height: 0,
            permalink: String::new(),
        };

        // Set up the mock Redis command that the scraper is expected to request.
        let cache_key = serde_json::to_vec(&date).expect("Couldn't serialize mock cache key");
        let cache_value =
            serde_json::to_vec(&comic_data).expect("Couldn't serialize mock cache value");
        let storage_cmd = MockCmd::new(Cmd::set(cache_key, cache_value), Ok(Value::Okay));

        // Max pool size is one, since only one connection is needed.
        let db = MockPool::new(1);
        if let Err((_, err)) = db.add(MockRedisConnection::new([storage_cmd])).await {
            panic!("Couldn't add mock DB connection to mock DB pool: {err}");
        };

        // The HTTP client shouldn't be used, so make the URLs empty.
        let scraper = InnerComicScraper::new(Some(db), String::new(), String::new());
        scraper
            .cache_data(&comic_data, &date)
            .await
            .expect("Failed to set comic data in cache");
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
        date_ymd: (i32, u32, u32),
        missing: bool,
        comic_data: (&str, &str, i32, i32),
    ) {
        let mock_server = MockServer::start().await;
        let date = NaiveDate::from_ymd_opt(date_ymd.0, date_ymd.1, date_ymd.2)
            .expect("Invalid test parameters");

        // The DB shouldn't be used, so use a pool with no connections.
        let db = Some(MockPool::new(0));
        let scraper =
            InnerComicScraper::new(db, mock_server.uri(), format!("{}/cdx", mock_server.uri()));

        let expected = ComicData {
            title: comic_data.0.into(),
            img_url: comic_data.1.into(),
            img_width: comic_data.2,
            img_height: comic_data.3,
            permalink: format!(
                "{}/{SRC_COMIC_PREFIX}{}",
                mock_server.uri(),
                date.format(SRC_DATE_FMT)
            ),
        };

        let date_str = date.format(SRC_DATE_FMT).to_string();
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
            .and(path(format!("/{SRC_COMIC_PREFIX}{date_str}")))
            .respond_with(response)
            .mount(&mock_server)
            .await;

        // Set up the mock server to return a bogus timestamp for the base URL, because this is
        // what the CDX URL is.
        Mock::given(method(Method::GET.as_str()))
            .and(path("/cdx"))
            .respond_with(ResponseTemplate::new(StatusCode::OK.as_u16()).set_body_string("2000"))
            .mount(&mock_server)
            .await;

        // The scraping should fail if and only if the server redirects.
        match scraper.scrape_data(&date).await {
            Ok(result) => {
                if missing {
                    panic!("Somehow scraped a missing comic");
                } else {
                    assert_eq!(result, expected, "Scraped the wrong comic data");
                }
            }
            Err(err) => {
                if !missing {
                    panic!("Failed to scrape comic data: {err}")
                }
            }
        };
    }

    #[test_case(GetCacheState::Fresh, true, true; "fresh retrieval")]
    #[test_case(GetCacheState::Stale, true, true; "stale retrieval, scrape works, storage works")]
    #[test_case(GetCacheState::Stale, true, false; "stale retrieval, scrape works, storage fails")]
    #[test_case(GetCacheState::Stale, false, true; "stale retrieval, scrape fails")]
    #[test_case(GetCacheState::NotFound, true, true; "empty cache, storage works")]
    #[test_case(GetCacheState::NotFound, true, false; "empty cache, storage fails")]
    #[test_case(GetCacheState::Fail, true, true; "cache retrieval fails, storage works")]
    #[test_case(GetCacheState::Fail, true, false; "cache retrieval fails, storage fails")]
    #[actix_web::test]
    /// Test multiple scenarios of data requested from the scraper.
    ///
    /// # Arguments
    /// * `retrieve_status` - Status for the cache retrieval
    /// * `scrape_works` - Whether scraping works
    /// * `storage_works` - Whether cache storage works
    async fn test_get_comic_data(
        retrieve_status: GetCacheState,
        scrape_works: bool,
        storage_works: bool,
    ) {
        // Set up the expected return values, and the entry to store in the mock cache.
        let date = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
        let comic_data = ComicData {
            title: String::new(),
            img_url: String::new(),
            img_width: 0,
            img_height: 0,
            permalink: String::new(),
        };
        let mut mock_scraper = MockInnerComicScraper::<MockPool>::default();

        // Mock cache retrieval.
        mock_scraper.expect_get_cached_data().return_once({
            let comic_data = comic_data.clone();
            move |_| match retrieve_status {
                GetCacheState::Fresh => Ok(Some((comic_data, true))),
                GetCacheState::Stale => Ok(Some((comic_data, false))),
                GetCacheState::NotFound => Ok(None),
                GetCacheState::Fail => Err(AppError::Scrape("Manual error".into())),
            }
        });

        // Mock cache storage.
        mock_scraper.expect_cache_data().return_once(move |_, _| {
            if storage_works {
                Ok(())
            } else {
                Err(AppError::Scrape("Manual error".into()))
            }
        });

        // Mock scraping.
        mock_scraper.expect_scrape_data().return_once({
            let comic_data = comic_data.clone();
            move |_| {
                if scrape_works {
                    Ok(comic_data)
                } else {
                    Err(AppError::Scrape("Manual error".into()))
                }
            }
        });

        let result = ComicScraper(mock_scraper)
            .get_comic_data(&date)
            .await
            .expect("Data retrieval from scraper crashed");
        assert_eq!(result, Some(comic_data), "Scraper returned the wrong data");
    }
}
