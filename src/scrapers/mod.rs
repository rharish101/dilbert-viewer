//! The module containing the scrapers
mod comics;
mod latest;
mod scraper;

// Re-export for convenience
pub(crate) use comics::{ComicData, ComicScraper};
pub(crate) use latest::LatestDateScraper;
pub(crate) use scraper::Scraper;
