// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Custom error definitions
use minify_html::Error as MinifyHtmlError;
use reqwest::Error as HttpError;
use sea_orm::DbErr;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MinificationError {
    /// Error minifying HTML
    #[error("Error minifying HTML: {0:?}")]
    Html(MinifyHtmlError),
    /// Error minifying CSS
    #[error("Error minifying CSS: {0}")]
    Css(String),
    /// Error minifying JS
    #[error("Error minifying JS: {0}")]
    Js(String),
}

impl From<MinifyHtmlError> for MinificationError {
    fn from(err: MinifyHtmlError) -> Self {
        Self::Html(err)
    }
}

#[derive(Error, Debug)]
/// All errors raised by the viewer
pub enum ViewerError {
    /// Errors when executing a DB query
    #[error("Database error: {0}")]
    Db(#[from] DbErr),
    /// Errors in parsing dates
    #[error("Error parsing date: {0}")]
    DateParse(#[from] chrono::format::ParseError),
    /// Errors in building HTML templates
    #[error("Error building HTML template: {0}")]
    Template(#[from] askama::Error),
    /// Errors in parsing UTF-8 from files
    #[error("Error parsing UTF-8: {0}")]
    Utf8(#[from] std::str::Utf8Error),
    /// Errors in minifying HTML/CSS
    #[error("Minification error: {0}")]
    Minify(#[from] MinificationError),
    /// Errors when no comic exists for a given date
    #[error("{0}")]
    NotFound(String),
}

/// Convenient alias for results with viewer errors
pub type ViewerResult<T> = Result<T, ViewerError>;

#[derive(Error, Debug)]
/// All errors raised by the scraper
pub enum ScraperError {
    /// Errors when executing a DB query
    #[error("Database error: {0}")]
    Db(#[from] DbErr),
    /// Errors when making HTTP requests
    #[error("HTTP client error: {0}")]
    Http(#[from] HttpError),
    /// Errors in HTML parsing
    #[error("HTML parse error: {0}")]
    HtmlParse(#[from] tl::errors::ParseError),
    /// Errors in scraping info from "dilbert.com"
    #[error("Scraping error: {0}")]
    Scrape(String),
    /// Errors when no comic exists for a given date
    #[error("{0}")]
    NotFound(String),
}

/// Convenient alias for results with scraper app errors
pub type ScraperResult<T> = Result<T, ScraperError>;
