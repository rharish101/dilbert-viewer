//! Custom error definitions
// This file is part of dilbert-viewer.
//
// Copyright (C) 2022  Harish Rajagopal <harish.rajagopals@gmail.com>
//
// dilbert-viewer is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// dilbert-viewer is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with dilbert-viewer.  If not, see <https://www.gnu.org/licenses/>.
use std::env;

use awc::error::{PayloadError, SendRequestError};
use deadpool_postgres::{BuildError, PoolError};
use minify_html::Error as MinifyHtmlError;
use thiserror::Error;
use tokio_postgres::error::Error as PgError;

#[derive(Error, Debug)]
/// Errors when initializing the database pool
pub enum DbInitError {
    /// Error reading the DB URL from the environment
    #[error("Missing environment variable for the database URL: {0}")]
    Env(#[from] env::VarError),
    /// Error parsing the DB URL
    #[error("Error parsing the database URL: {0}")]
    Db(#[from] PgError),
    /// Error in building an SSL connector
    #[error("Error building an SSL connector: {0}")]
    Tls(#[from] native_tls::Error),
    /// Error in building the DB connection pool
    #[error("Error building the database connection pool: {0}")]
    Pool(#[from] BuildError),
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
    /// Errors in getting a connection from the DB pool, or when executing a DB query
    #[error("Database pool error: {0}")]
    Db(#[from] PoolError),
    /// Errors in initializing the DB
    #[error("Error initializing the database: {0}")]
    DbInit(#[from] DbInitError),
    /// Errors when building an HTTP client, or when making HTTP requests
    #[error("HTTP client error: {0}")]
    Http(HttpError),
    /// Errors in parsing dates
    #[error("Error parsing date: {0}")]
    DateParse(#[from] chrono::format::ParseError),
    /// Errors in regex pattern syntax, or when parsing strings using regex
    #[error("Regex error: {0}")]
    Regex(regex::Error, String),
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

impl From<PgError> for AppError {
    fn from(err: PgError) -> Self {
        Self::Db(PoolError::Backend(err))
    }
}

/// Convenient alias for results with viewer app errors
pub type AppResult<T> = Result<T, AppError>;
