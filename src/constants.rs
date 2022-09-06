//! All constants used by the viewer
// ==================================================
// Date formats
// ==================================================
/// Date of the first ever Dilbert comic
pub const FIRST_COMIC: &str = "1989-04-16";
/// Date format used for URLs on "dilbert.com"
pub const DATE_FMT: &str = "%Y-%m-%d";
/// Date format used for display with the comic on "dilbert.com"
pub const ALT_DATE_FMT: &str = "%A %B %d, %Y";

// ==================================================
// Parameters for scraping from "dilbert.com"
// ==================================================
/// Timeout (in seconds) for establishing a connection
pub const FETCH_TIMEOUT: u64 = 3;

// ==================================================
// Parameters for caching to the database
// ==================================================
/// Limit for connections to the cache database
// Heroku's free tier limit is 20.
pub const MAX_DB_CONN: usize = 19;
/// Timeout (in seconds) for a single database operation
pub const DB_TIMEOUT: u64 = 3;
/// Limit (in no. of comics) for the comics cache in the database
// Heroku's free tier limit is 10,000 rows in a database with max. size 1GB. Note that apart from
// this, we have the latest date table, which always has exactly one row.
pub const CACHE_LIMIT: f32 = 9900.0;
/// No. of hrs after scraping the latest date when it is to be scraped again
pub const LATEST_DATE_REFRESH: f64 = 2.0;

// ==================================================
// Miscellaneous
// ==================================================
/// URL prefix for each comic on "dilbert.com"
pub const SRC_PREFIX: &str = "https://dilbert.com/strip/";
/// Link to the public version of this repo
// Mainly for publicity :P
pub const REPO: &str = "https://github.com/rharish101/dilbert-viewer";
/// URL path for static files
// This is set to root as it's easy to serve robots.txt by keeping it in static.
pub const STATIC_URL: &str = "/";
