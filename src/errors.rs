// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Custom error definitions
use std::env;

use awc::error::{PayloadError, SendRequestError};
use deadpool_redis::{BuildError, ConfigError, PoolError};
use minify_html::Error as MinifyHtmlError;
use thiserror::Error;

#[derive(Error, Debug)]
/// Errors when initializing the database pool
pub enum DbInitError {
    /// Error reading the DB URL from the environment
    #[error("Missing environment variable for the database URL: {0}")]
    Env(#[from] env::VarError),
    /// Invalid Redis URL
    #[error("Error in the Redis URL: {0}")]
    Config(#[from] ConfigError),
    /// Error initializing the DB pool
    #[error("Error initializing the database pool: {0}")]
    Build(#[from] BuildError),
}

#[derive(Error, Debug)]
pub enum HttpError {
    /// Error sending a request
    #[error("Error sending request: {0}")]
    SendRequest(#[from] SendRequestError),
    /// Error processing the response payload
    #[error("Error parsing payload: {0}")]
    Payload(#[from] PayloadError),
}

#[derive(Error, Debug)]
pub enum MinificationError {
    /// Error minifying HTML
    #[error("Error minifying HTML: {0:?}")]
    Html(MinifyHtmlError),
    /// Error minifying CSS
    #[error("Error minifying CSS: {0}")]
    Css(String),
}

impl From<MinifyHtmlError> for MinificationError {
    fn from(err: MinifyHtmlError) -> Self {
        Self::Html(err)
    }
}

#[derive(Error, Debug)]
/// All errors raised by the viewer app
pub enum AppError {
    /// Errors when acquiring a connection from the DB pool
    #[error("Error acquiring DB connection: {0}")]
    Pool(#[from] PoolError),
    /// Errors when executing a DB query
    #[error("Database error: {0}")]
    Db(#[from] redis::RedisError),
    /// Errors when serializing/deserializing a DB query argument/result
    #[error("(De)serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    /// Errors when building an HTTP client, or when making HTTP requests
    #[error("HTTP client error: {0}")]
    Http(HttpError),
    /// Errors in parsing dates
    #[error("Error parsing date: {0}")]
    DateParse(#[from] chrono::format::ParseError),
    /// Errors in HTML parsing
    #[error("HTML parse error: {0}")]
    HtmlParse(#[from] tl::errors::ParseError),
    /// Errors in building HTML templates
    #[error("Error building HTML template: {0}")]
    Template(#[from] askama::Error),
    /// Errors in parsing UTF-8 from files
    #[error("Error parsing UTF-8: {0}")]
    Utf8(#[from] std::str::Utf8Error),
    /// Errors in minifying HTML/CSS
    #[error("Minification error: {0}")]
    Minify(#[from] MinificationError),
    /// Miscellaneous internal errors
    #[error("Internal error: {0}")]
    Internal(String),
    /// Errors in scraping info from "dilbert.com"
    #[error("Scraping error: {0}")]
    Scrape(String),
    /// Errors when no comic exists for a given date
    #[error("{0}")]
    NotFound(String),
}

impl<E> From<E> for AppError
where
    E: Into<HttpError>,
{
    fn from(err: E) -> Self {
        Self::Http(err.into())
    }
}

/// Convenient alias for results with viewer app errors
pub type AppResult<T> = Result<T, AppError>;
