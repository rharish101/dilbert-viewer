//! Utilities for the viewer app
use chrono::{format::ParseResult, NaiveDate, Utc};

/// Return the current date.
///
/// The timezone is fixed to UTC so that the code is independent of local time.
pub(crate) fn curr_date() -> NaiveDate {
    Utc::today().naive_utc()
}

/// Convert the date string (assumed in UTC) to a `chrono::NaiveDate` struct.
///
/// # Arguments
/// * `date_str` - The input date
/// * `fmt` - The format of the input date
pub(crate) fn str_to_date(date: &str, fmt: &str) -> ParseResult<NaiveDate> {
    NaiveDate::parse_from_str(date, fmt)
}
