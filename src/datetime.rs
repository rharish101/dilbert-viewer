// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Datetime utilities for the viewer app
use jiff::civil::Date;

/// Convert the date string (assumed in UTC) to a `jiff::civil::Date` struct.
///
/// # Arguments
/// * `date` - The input date
/// * `fmt` - The format of the input date
pub fn str_to_date(date: &str, fmt: &str) -> Result<Date, jiff::Error> {
    Date::strptime(fmt, date)
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
    /// Note: These test parameters use Jiff's native types, so out-of-range values like a 5-digit
    /// year aren't even representable here (they're rejected at the HTTP boundary).
    ///
    /// # Arguments
    /// * `date` - The input date as a string
    /// * `fmt` - The format of the input date
    /// * `year` - The year of the input date
    /// * `month` - The month of the input date
    /// * `day` - The day of the input date
    fn test_str_to_date(date: &str, fmt: &str, year: i16, month: u8, day: u8) {
        let result = str_to_date(date, fmt).ok();
        let expected = Date::new(year, month as i8, day as i8).ok();
        assert_eq!(result, expected);
    }
}
