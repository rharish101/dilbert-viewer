//! Contains structs for HTML templates
use askama::Template;

use crate::scrapers::ComicData;

/// The main template for a comic
#[derive(Template)]
#[template(path = "comic.html")]
pub(crate) struct ComicTemplate<'a> {
    /// The scraped comic data
    pub data: &'a ComicData,

    // All date formats should conform to the format given by `crate::constants::DATE_FMT`
    /// The date of the comic
    pub date: &'a str,
    /// The date of the first comic
    pub first_comic: &'a str,
    /// The date of the previous comic, if available
    pub previous_comic: &'a str,
    /// The date of the next comic, if available
    pub next_comic: &'a str,

    /// Whether to disable navigation to previous comics
    pub disable_left_nav: bool,
    /// Whether to disable navigation to next comics
    pub disable_right_nav: bool,
    /// Link to the original source comic
    pub permalink: &'a str,
    /// Link to the repo where this code is hosted
    pub repo: &'a str,
}

/// The template for a 404 not found page
#[derive(Template)]
#[template(path = "not_found.html")]
pub(crate) struct NotFoundTemplate<'a> {
    /// The date of the requested comic, if available
    pub date: Option<&'a str>,
    /// Link to the repo where this code is hosted
    pub repo: &'a str,
}

/// The template for a 500 internal server error page
#[derive(Template)]
#[template(path = "error.html")]
pub(crate) struct ErrorTemplate<'a> {
    /// The error message of the interval server error
    pub error: &'a str,
    /// Link to the repo where this code is hosted
    pub repo: &'a str,
}
