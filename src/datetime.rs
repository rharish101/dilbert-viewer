// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Datetime utilities for the viewer app
use chrono::{format::ParseResult, NaiveDate};

/// Convert the date string (assumed in UTC) to a `chrono::NaiveDate` struct.
///
/// # Arguments
/// * `date` - The input date
/// * `fmt` - The format of the input date
pub fn str_to_date(date: &str, fmt: &str) -> ParseResult<NaiveDate> {
    NaiveDate::parse_from_str(date, fmt)
}

#[cfg(test)]
mod tests {
    use super::*;

    use test_case::test_case;

    #[test_case("2000-01-01", "%Y-%m-%d", 2000, 1, 1; "yyyy-mm-dd valid")]
    #[test_case("2000-01-00", "%Y-%m-%d", 2000, 1, 0; "yyyy-mm-dd invalid")]
    #[test_case("Saturday January 01, 2000", "%A %B %d, %Y", 2000, 1, 1; "day MM dd, yyyy valid")]
    #[test_case("Sunday January 01, 2000", "%A %B %d, %Y", 0, 0, 0; "day MM dd, yyyy invalid")]
    /// Test the string to date converter.
    ///
    /// # Arguments
    /// * `date` - The input date as a string
    /// * `fmt` - The format of the input date
    /// * `year` - The year of the input date
    /// * `month` - The month of the input date
    /// * `day` - The day of the input date
    fn test_str_to_date(date: &str, fmt: &str, year: i32, month: u32, day: u32) {
        let result = str_to_date(date, fmt).ok();
        let expected = NaiveDate::from_ymd_opt(year, month, day);
        assert_eq!(result, expected);
    }
}
