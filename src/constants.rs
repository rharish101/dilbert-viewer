// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! All constants used by the viewer
// ==================================================
// Date formats
// ==================================================
/// Date of the first ever Dilbert comic
pub const FIRST_COMIC: &str = "1989-04-16";
/// Date of the last available Dilbert comic
pub const LAST_COMIC: &str = "2023-03-12";
/// Date format used for URLs on "dilbert.com"
pub const SRC_DATE_FMT: &str = "%Y-%m-%d";
/// Date format used for display with the comic on "dilbert.com"
pub const DISP_DATE_FMT: &str = "%A %B %d, %Y";

// ==================================================
// Parameters for scraping from the Wayback Machine
// ==================================================
/// Max number of dates scraped concurrently
pub const MAX_CONC_SCRAPES: usize = 5;
/// Delay between each scrape
pub const SCRAPE_DELAY: u64 = 2;
/// Timeout (in seconds) for getting a response
pub const RESP_TIMEOUT: u64 = 30;
/// User agent string (required by Wayback Machine) in the format `{tool-name}/{version}`
pub const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
/// Base URL for Wayback Machine lookups
pub const ARC_BASE_URL: &str =
    "https://web.archive.org/web/{timestamp}/https://dilbert.com/strip/{date}";
/// URL for archive.org CDX API
// Docs: https://github.com/internetarchive/wayback/tree/master/wayback-cdx-server
pub const CDX_URL: &str = "https://web.archive.org/cdx/search/cdx?url=https://dilbert.com/strip/{date}&fl=timestamp&filter=statuscode:^2&limit=-1&to=20230312";
/// Fallback timestamp for the Wayback Machine, if the CDX API fetch fails
pub const CDX_TIMESTAMP_FALLBACK: &str = "2018";

// ==================================================
// Parameters related to the database
// ==================================================
/// Limit for connections to the database
pub const MAX_DB_CONN: u32 = 19;
/// Timeout (in seconds) for a single database operation
pub const DB_TIMEOUT: u64 = 5;
/// Module path prefix for the schema registry, covering all entities
pub const ENTITY_PREFIX: &str = concat!(env!("CARGO_PKG_NAME"), "::entities::*");

// ==================================================
// Miscellaneous
// ==================================================
/// URL path for static files
// This is set to root as it's easy to serve robots.txt by keeping it in static.
pub const STATIC_URL: &str = "/";
/// Location of static files
pub const STATIC_DIR: &str = "static/";
/// Content security policy
pub const CSP: &str = "\
    default-src 'none';\
    img-src assets.amuniversal.com dilbert.com web.archive.org;\
    style-src 'self' cdn.jsdelivr.net;\
    script-src 'self';\
    frame-ancestors 'none'";

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::Path;

    use actix_web::middleware::DefaultHeaders;
    use content_security_policy as csp;
    use jiff::civil::Date;

    #[test]
    /// Test whether the first comic date is in the expected format.
    fn test_first_comic_format() {
        assert!(
            Date::strptime(SRC_DATE_FMT, FIRST_COMIC).is_ok(),
            "FIRST_COMIC doesn't match SRC_DATE_FMT"
        )
    }

    #[test]
    /// Test whether the date format for "dilbert.com" is valid.
    fn test_src_date_format() {
        // This should error if the format is invalid.
        jiff::fmt::strtime::format(SRC_DATE_FMT, Date::new(2000, 1, 1).unwrap())
            .expect("SRC_DATE_FMT is not a valid format");
    }

    #[test]
    /// Test whether the date format used for displaying is valid.
    fn test_disp_date_format() {
        // This should error if the format is invalid.
        jiff::fmt::strtime::format(DISP_DATE_FMT, Date::new(2000, 1, 1).unwrap())
            .expect("DISP_DATE_FMT is not a valid format");
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
