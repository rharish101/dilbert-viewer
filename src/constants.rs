//! All constants used by the viewer
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

// ==================================================
// Date formats
// ==================================================
/// Date of the first ever Dilbert comic
pub const FIRST_COMIC: &str = "1989-04-16";
/// Date format used for URLs on "dilbert.com"
pub const SRC_DATE_FMT: &str = "%Y-%m-%d";
/// Date format used for display with the comic on "dilbert.com"
pub const DISP_DATE_FMT: &str = "%A %B %d, %Y";

// ==================================================
// Parameters for scraping from "dilbert.com"
// ==================================================
/// Timeout (in seconds) for getting a response
pub const RESP_TIMEOUT: u64 = 5;

// ==================================================
// Parameters for caching to the database
// ==================================================
/// Limit for connections to the cache database
// Heroku's free tier limit is 20.
pub const MAX_DB_CONN: usize = 19;
/// Timeout (in seconds) for a single database operation
pub const DB_TIMEOUT: u64 = 3;
/// No. of hrs after scraping the latest date when it is to be scraped again
pub const LATEST_DATE_REFRESH: i64 = 2;

// ==================================================
// Miscellaneous
// ==================================================
/// URL prefix for each comic on "dilbert.com"
pub const SRC_PREFIX: &str = "https://dilbert.com/strip/";
/// Default port when one isn't specified
// This is Heroku's default port when running locally
pub const PORT: u16 = 5000;
/// Link to the public version of this app
// Used in the OpenGraph tags
pub const APP_URL: &str = "https://dilbert-viewer.herokuapp.com/";
/// Link to the public version of this repo
// Mainly for publicity :P
pub const REPO_URL: &str = "https://github.com/rharish101/dilbert-viewer";
/// URL path for static files
// This is set to root as it's easy to serve robots.txt by keeping it in static.
pub const STATIC_URL: &str = "/";
/// Location of static files
pub const STATIC_DIR: &str = "static/";
/// Content security policy
pub const CSP: &str = "\
    default-src 'none';\
    img-src 'self' assets.amuniversal.com;\
    style-src 'self' cdn.jsdelivr.net";

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::Path;

    use actix_web::middleware::DefaultHeaders;
    use chrono::NaiveDate;
    use content_security_policy as csp;

    #[test]
    /// Test whether the first comic date is in the expected format.
    fn test_first_comic_format() {
        assert!(
            NaiveDate::parse_from_str(FIRST_COMIC, SRC_DATE_FMT).is_ok(),
            "FIRST_COMIC doesn't match SRC_DATE_FMT"
        )
    }

    #[test]
    /// Test whether the date format for "dilbert.com" is valid.
    fn test_src_date_format() {
        NaiveDate::from_ymd_opt(2000, 1, 1)
            .unwrap()
            .format(SRC_DATE_FMT)
            // This should panic at `.to_string` if the format is invalid.
            .to_string();
    }

    #[test]
    /// Test whether the date format used for displaying is valid.
    fn test_disp_date_format() {
        NaiveDate::from_ymd_opt(2000, 1, 1)
            .unwrap()
            .format(DISP_DATE_FMT)
            // This should panic at `.to_string` if the format is invalid.
            .to_string();
    }

    #[test]
    /// Test whether the directory of static files exists.
    fn test_if_static_dir_exists() {
        assert!(
            Path::new(STATIC_DIR).exists(),
            "Static directory doesn't exist"
        );
    }

    #[test]
    /// Test whether the content security policy (CSP) is a valid header value.
    ///
    /// Note that this doesn't check if the CSP follows the CSP format.
    fn test_content_security_policy_header_format() {
        // This panics if the *header* format is invalid (not CSP format).
        DefaultHeaders::new().add(("Content-Security-Policy", CSP));

        let policy = csp::Policy::parse(
            CSP,
            csp::PolicySource::Header,
            csp::PolicyDisposition::Enforce,
        );
        assert!(policy.is_valid(), "CSP is invalid");
        // See if at least one directive exists.
        assert!(!policy.directive_set.is_empty(), "CSP has no directives");
    }
}
