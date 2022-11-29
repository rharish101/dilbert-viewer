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
use async_trait::async_trait;
use awc::http::StatusCode;
use chrono::NaiveDate;
use html_escape::decode_html_entities;
use log::{error, info};
use serde::{Deserialize, Serialize};
use tl::{parse as parse_html, Bytes, Node, ParserOptions};

use crate::client::HttpClient;
use crate::constants::{SRC_COMIC_PREFIX, SRC_DATE_FMT};
use crate::db::{RedisPool, SerdeAsyncCommands};
use crate::errors::{AppError, AppResult};
use crate::scrapers::Scraper;

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug)]
pub struct ComicData {
    /// The title of the comic
    pub title: String,

    /// The URL to the comic image
    pub img_url: String,

    /// The width of the image
    pub img_width: i32,

    /// The height of the image
    pub img_height: i32,
}

/// Struct for a comic scraper
///
/// This scraper takes a date as input and returns the info about the comic.
pub struct ComicScraper {}

impl ComicScraper {
    /// Initialize a comics scraper.
    pub fn new() -> Self {
        Self {}
    }

    /// Retrieve the data for the requested comic.
    ///
    /// # Arguments
    /// * `db` - The pool of connections to the DB
    /// * `http_client` - The HTTP client for scraping from the source
    /// * `date` - The date of the requested comic
    pub async fn get_comic_data(
        &self,
        db: &Option<impl RedisPool>,
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
        db: &Option<impl RedisPool>,
        date: &NaiveDate,
    ) -> AppResult<Option<(ComicData, bool)>> {
        let mut conn = if let Some(db) = db {
            db.get().await?
        } else {
            return Ok(None);
        };

        // None would mean that the comic for this date wasn't cached, or the date is invalid (i.e.
        // it would redirect to the homepage).
        let comic_data: Option<ComicData> = conn.get(date).await?;
        Ok(comic_data.map(|comic_data| (comic_data, true)))
    }

    /// Cache the comic data into the database.
    async fn cache_data(
        &self,
        db: &Option<impl RedisPool>,
        comic_data: &ComicData,
        date: &NaiveDate,
    ) -> AppResult<()> {
        let mut conn = if let Some(db) = db {
            db.get().await?
        } else {
            return Ok(());
        };

        conn.set(date, comic_data).await?;
        info!("Successfully cached data for {} in cache", date);
        Ok(())
    }

    /// Scrape the comic data of the requested date from the source.
    async fn scrape_data(
        &self,
        http_client: &HttpClient,
        date: &NaiveDate,
    ) -> AppResult<ComicData> {
        let path = format!("{}{}", SRC_COMIC_PREFIX, date.format(SRC_DATE_FMT));
        let mut resp = http_client.get(&path).send().await?;
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
            title,
            img_url,
            img_width,
            img_height,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use actix_web::http::{Method, StatusCode};
    use deadpool_redis::redis::{Cmd, Value};
    use redis_test::{IntoRedisValue, MockCmd, MockRedisConnection};
    use test_case::test_case;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    use crate::db::mock::MockPool;
    use crate::scrapers::scraper::mock::GetCacheState;

    /// Path to the directory where test scraping files are stored
    const SCRAPING_TEST_CASE_PATH: &str = "testdata/scraping";

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
            panic!("Couldn't add mock DB connection to mock DB pool: {}", err);
        };

        let scraper = ComicScraper::new();
        let result = scraper
            .get_cached_data(&Some(db), &date)
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
        };

        // Set up the mock Redis command that the scraper is expected to request.
        let cache_key = serde_json::to_vec(&date).expect("Couldn't serialize mock cache key");
        let cache_value =
            serde_json::to_vec(&comic_data).expect("Couldn't serialize mock cache value");
        let storage_cmd = MockCmd::new(Cmd::set(cache_key, cache_value), Ok(Value::Okay));

        // Max pool size is one, since only one connection is needed.
        let db = MockPool::new(1);
        if let Err((_, err)) = db.add(MockRedisConnection::new([storage_cmd])).await {
            panic!("Couldn't add mock DB connection to mock DB pool: {}", err);
        };

        let scraper = ComicScraper::new();
        scraper
            .cache_data(&Some(db), &comic_data, &date)
            .await
            .expect("Failed to set comic data in cache");
    }

    #[test_case((2000, 1, 1), false, ("", "https://assets.amuniversal.com/bdc8a4d06d6401301d80001dd8b71c47", 900, 266); "without title")]
    #[test_case((2020, 1, 1), false, ("Rfp Process", "https://assets.amuniversal.com/7c2789d004020138d860005056a9545d", 900, 280); "with title")]
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
        let http_client = HttpClient::new(mock_server.uri());
        let date = NaiveDate::from_ymd_opt(date_ymd.0, date_ymd.1, date_ymd.2)
            .expect("Invalid test parameters");
        let scraper = ComicScraper::new();

        let expected = ComicData {
            title: comic_data.0.into(),
            img_url: comic_data.1.into(),
            img_width: comic_data.2,
            img_height: comic_data.3,
        };

        let date_str = date.format(SRC_DATE_FMT).to_string();
        let response = if missing {
            // "dilbert.com" uses 302 FOUND to inform that the comic is missing.
            // Response body shouldn't matter, so keep it empty.
            ResponseTemplate::new(StatusCode::FOUND.as_u16())
        } else {
            let html =
                tokio::fs::read_to_string(format!("{}/{}.html", SCRAPING_TEST_CASE_PATH, date_str))
                    .await
                    .expect("Couldn't read test page for scraping");
            ResponseTemplate::new(StatusCode::OK.as_u16()).set_body_string(html)
        };

        // Set up the mock server to return the pre-fetched "dilbert.com" response for the given date.
        Mock::given(method(Method::GET.as_str()))
            .and(path(format!("/{}{}", SRC_COMIC_PREFIX, date_str)))
            .respond_with(response)
            .mount(&mock_server)
            .await;

        // The scraping should fail if and only if the server redirects.
        if let Ok(result) = scraper.scrape_data(&http_client, &date).await {
            if missing {
                panic!("Somehow scraped a missing comic");
            } else {
                assert_eq!(result, expected, "Scraped the wrong comic data");
            }
        } else if !missing {
            panic!("Failed to scrape comic data");
        };
    }
}
