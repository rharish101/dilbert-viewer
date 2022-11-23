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
pub const MAX_DB_CONN: u32 = 19;
/// Timeout (in seconds) for a single database operation
pub const DB_TIMEOUT: u64 = 3;
/// Limit (in no. of comics) for the comics cache in the database
// Heroku's free tier limit is 10,000 rows in a database with max. size 1GB. Note that apart from
// this, we have the latest date table, which always has exactly one row.
pub const CACHE_LIMIT: u64 = 9900;
/// No. of hrs after scraping the latest date when it is to be scraped again
pub const LATEST_DATE_REFRESH: i64 = 2;

// ==================================================
// Miscellaneous
// ==================================================
/// URL prefix for each comic on "dilbert.com"
pub const SRC_PREFIX: &str = "https://dilbert.com/strip/";
/// Default port when one isn't specified
// This is Heroku's default port when running locally
pub const PORT: &str = "5000";
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
