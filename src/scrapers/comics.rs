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
use awc::{http::StatusCode, Client as HttpClient};
use chrono::NaiveDate;
use deadpool_redis::{redis::AsyncCommands, Pool as RedisPool};
use html_escape::decode_html_entities;
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use tl::{parse as parse_html, Bytes, Node, ParserOptions};

use crate::constants::{SRC_DATE_FMT, SRC_PREFIX};
use crate::errors::{AppError, AppResult};
use crate::scrapers::Scraper;

#[derive(Deserialize, Serialize)]
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
        db: &Option<RedisPool>,
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
        db: &Option<RedisPool>,
        date: &NaiveDate,
    ) -> AppResult<Option<(ComicData, bool)>> {
        let mut conn = if let Some(db) = db {
            db.get().await?
        } else {
            return Ok(None);
        };

        let raw_data: Option<Vec<u8>> = conn.get(serde_json::to_vec(date)?).await?;
        debug!("Retrieved raw data from DB for date: {}", date);

        Ok(if let Some(raw_data) = raw_data {
            let comic_data: ComicData = serde_json::from_slice(raw_data.as_slice())?;
            Some((comic_data, true))
        } else {
            // This means that the comic for this date wasn't cached, or the date is invalid (i.e.
            // it would redirect to the homepage).
            None
        })
    }

    /// Cache the comic data into the database.
    async fn cache_data(
        &self,
        db: &Option<RedisPool>,
        comic_data: &ComicData,
        date: &NaiveDate,
    ) -> AppResult<()> {
        let mut conn = if let Some(db) = db {
            db.get().await?
        } else {
            return Ok(());
        };

        conn.set(serde_json::to_vec(date)?, serde_json::to_vec(comic_data)?)
            .await?;

        info!("Successfully cached data for {} in cache", date);
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
            title,
            img_url,
            img_width,
            img_height,
        })
    }
}
