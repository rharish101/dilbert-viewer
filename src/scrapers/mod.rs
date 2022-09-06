//! The module containing the scrapers
mod comics;
mod latest;
mod scraper;

// Re-export for convenience.
pub use comics::{ComicData, ComicScraper};
pub use latest::LatestDateScraper;
pub use scraper::Scraper;
