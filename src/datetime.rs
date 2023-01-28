// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Datetime utilities for the viewer app
use chrono::{format::ParseResult, NaiveDate, NaiveDateTime};

// Used to mock the current datetime.
#[cfg(test)]
pub mod mock {
    use super::*;

    use chrono::{DateTime, Utc};
    use faketime::{millis_tempfile, unix_time_as_millis};
    use tempfile::TempPath;
    use tracing::warn;

    pub struct MockUtc {}
    impl MockUtc {
        pub fn now() -> DateTime<Utc> {
            let possibly_fake_time = unix_time_as_millis();
            if let Some(datetime) = i64::try_from(possibly_fake_time)
                .ok()
                .and_then(NaiveDateTime::from_timestamp_millis)
            {
                DateTime::from_utc(datetime, Utc)
            } else {
                warn!("Couldn't use mocked time due to integer overflow");
                Utc::now()
            }
        }
    }

    pub fn mock_time_file(datetime: &DateTime<Utc>) -> TempPath {
        match u64::try_from(datetime.timestamp_millis()) {
            Ok(millis) => millis_tempfile(millis).expect("Couldn't set mock time"),
            Err(err) => panic!("Couldn't set mock time: {err}"),
        }
    }
}

#[cfg(test)]
use self::mock::MockUtc as Utc;
#[cfg(not(test))]
use chrono::Utc;

/// Return the current date.
///
/// The timezone is fixed to UTC so that the code is independent of local time.
pub fn curr_date() -> NaiveDate {
    Utc::now().date_naive()
}

/// Return the current datetime.
///
/// The timezone is fixed to UTC so that the code is independent of local time.
pub fn curr_datetime() -> NaiveDateTime {
    Utc::now().naive_utc()
}

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
    use super::mock::mock_time_file;
    use super::*;

    use chrono::DateTime;
    use test_case::test_case;

    #[test_case(1970, 1, 1; "unix epoch")]
    #[test_case(2000, 2, 29; "feb 29")]
    #[test_case(2077, 1, 1; "post-2038")]
    /// Test the function that returns the current date.
    ///
    /// This function mocks the current date to test it.
    fn test_curr_date(year: i32, month: u32, day: u32) {
        let expected = NaiveDate::from_ymd_opt(year, month, day).expect("Invalid test parameters");
        let mock_datetime =
            DateTime::<chrono::Utc>::from_utc(expected.and_hms_opt(0, 0, 0).unwrap(), chrono::Utc);

        let faketime_file = mock_time_file(&mock_datetime);
        faketime::enable(&faketime_file);

        let returned = curr_date();
        assert_eq!(returned, expected);
    }

    #[test_case(1970, 1, 1, 0, 0, 0; "unix epoch")]
    #[test_case(2000, 2, 29, 23, 59, 59; "feb 29")]
    #[test_case(2077, 1, 1, 0, 0, 1; "post-2038")]
    /// Test the function that returns the current date.
    ///
    /// This function mocks the current date to test it.
    fn test_curr_datetime(year: i32, month: u32, day: u32, hour: u32, min: u32, sec: u32) {
        let expected = NaiveDate::from_ymd_opt(year, month, day)
            .expect("Invalid test parameters")
            .and_hms_opt(hour, min, sec)
            .expect("Invalid test parameters");
        let mock_datetime = DateTime::<chrono::Utc>::from_utc(expected, chrono::Utc);

        let faketime_file = mock_time_file(&mock_datetime);
        faketime::enable(&faketime_file);

        let returned = curr_datetime();
        assert_eq!(returned, expected);
    }

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
