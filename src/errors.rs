//! Custom error definitions
use std::env;

use askama::Error as TemplateError;
use chrono::format::ParseError;
use deadpool_postgres::{BuildError, PoolError};
use native_tls::Error as TlsError;
use regex::Error as RegexError;
use reqwest::Error as HttpError;
use tokio_postgres::error::Error as PgError;

#[derive(Debug)]
/// Errors when initializing the database pool
pub enum DbInitError {
    /// Error reading the DB URL from the environment
    Env(env::VarError),
    /// Error parsing the DB URL
    Db(PgError),
    /// Error in building an SSL connector
    Tls(TlsError),
    /// Error in building the DB connection pool
    Pool(BuildError),
}

impl From<env::VarError> for DbInitError {
    fn from(err: env::VarError) -> Self {
        Self::Env(err)
    }
}

impl From<PgError> for DbInitError {
    fn from(err: PgError) -> Self {
        Self::Db(err)
    }
}

impl From<TlsError> for DbInitError {
    fn from(err: TlsError) -> Self {
        Self::Tls(err)
    }
}

impl From<BuildError> for DbInitError {
    fn from(err: BuildError) -> Self {
        Self::Pool(err)
    }
}

#[derive(Debug)]
/// All errors raised by the viewer app
pub enum AppError {
    /// Errors in getting a connection from the DB pool, or when executing a DB query
    Db(PoolError),
    /// Errors in initializing the DB
    DbInit(DbInitError),
    /// Errors when building an HTTP client, or when making HTTP requests
    Http(HttpError),
    /// Errors in parsing dates
    DateParse(ParseError),
    /// Errors in regex pattern syntax, or when parsing strings using regex
    Regex(RegexError, String),
    /// Errors in building HTML templates
    Template(TemplateError),
    /// Miscellaneous internal errors
    Internal(String),
    /// Errors in scraping info from "dilbert.com"
    Scrape(String),
    /// Errors when no comic exists for a given date
    NotFound(String),
}

impl From<PoolError> for AppError {
    fn from(err: PoolError) -> Self {
        Self::Db(err)
    }
}

impl From<DbInitError> for AppError {
    fn from(err: DbInitError) -> Self {
        Self::DbInit(err)
    }
}

impl From<PgError> for AppError {
    fn from(err: PgError) -> Self {
        Self::Db(PoolError::Backend(err))
    }
}

impl From<HttpError> for AppError {
    fn from(err: HttpError) -> Self {
        Self::Http(err)
    }
}

impl From<ParseError> for AppError {
    fn from(err: ParseError) -> Self {
        Self::DateParse(err)
    }
}

impl From<TemplateError> for AppError {
    fn from(err: TemplateError) -> Self {
        Self::Template(err)
    }
}

/// Convenient alias for results with viewer app errors
pub type AppResult<T> = Result<T, AppError>;
