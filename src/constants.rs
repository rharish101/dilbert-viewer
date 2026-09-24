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
// HTTP response header values
// ==================================================
/// "Content-Security-Policy" header value
pub const CSP_VALUE: &str = "\
    default-src 'none';\
    img-src assets.amuniversal.com dilbert.com web.archive.org;\
    style-src 'self' cdn.jsdelivr.net;\
    script-src 'self';\
    frame-ancestors 'none'";
/// "Cache-Control" header value
// Comic pages and assets rarely change within a day, so 24 * 60 * 60 = 86400 s is safe.
pub const CACHE_CONTROL_VALUE: &str = "public, max-age=86400";
/// "Cache-Control" header value for versioned static assets
// A URL containing the crate version is never modified once served, so it can be cached for a
// year (365 * 24 * 60 * 60 = 31536000 s) and marked immutable (MDN's cache-busting pattern).
pub const CACHE_CONTROL_VALUE_IMMUTABLE: &str = "public, max-age=31536000, immutable";
/// "Referrer-Policy" header value
// The viewed comic's URL is never sent to external sites, but internal navigation is still logged.
pub const REFERRER_POLICY_VALUE: &str = "same-origin";

// ==================================================
// Miscellaneous
// ==================================================
/// URL path for static files
// This is set to root as it's easy to serve robots.txt by keeping it in static.
pub const STATIC_URL: &str = "/";

#[cfg(test)]
mod tests {
    use super::*;

    use actix_web::middleware::DefaultHeaders;
    use content_security_policy as csp;
    use jiff::civil::Date;
    use test_case::test_case;

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

    #[test_case("Content-Security-Policy", CSP_VALUE; "Content-Security-Policy")]
    #[test_case("Cache-Control", CACHE_CONTROL_VALUE; "Cache-Control")]
    #[test_case("Cache-Control", CACHE_CONTROL_VALUE_IMMUTABLE; "Cache-Control (immutable)")]
    #[test_case("Referrer-Policy", REFERRER_POLICY_VALUE; "Referrer-Policy")]
    /// Test whether a header's format is valid (*syntactically*, not semantically).
    ///
    /// # Arguments
    /// * `name` - The header name
    /// * `value` - The header value
    fn test_header_format(name: &str, value: &str) {
        // This panics on invalid formats.
        DefaultHeaders::new().add((name, value));
    }

    #[test]
    /// Test whether the content security policy (CSP) is a valid header value.
    ///
    /// Note that this doesn't check if the CSP follows the CSP format.
    fn test_content_security_policy_header_value() {
        let policy = csp::Policy::parse(
            CSP_VALUE,
            csp::PolicySource::Header,
            csp::PolicyDisposition::Enforce,
        );
        assert!(policy.is_valid(), "CSP is invalid");
        // See if at least one directive exists.
        assert!(!policy.directive_set.is_empty(), "CSP has no directives");
    }
}
