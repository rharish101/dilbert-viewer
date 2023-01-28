// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The module containing the scrapers
mod comics;
mod latest;
mod scraper;

use mockall_double::double;

// Re-export for convenience.
pub use comics::ComicData;
#[double]
pub use comics::ComicScraper;
#[double]
pub use latest::LatestDateScraper;
pub use scraper::Scraper;
