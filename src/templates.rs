// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Contains structs for HTML templates
use askama::Template;

use crate::scraper::ComicData;

/// The main template for a comic
#[derive(Template, Debug)]
#[template(path = "comic.html")]
pub struct ComicTemplate<'a> {
    /// The scraped comic data
    pub data: &'a ComicData,
    /// The date of the comic, formatted for display
    pub date_disp: &'a str,

    // All date formats should conform to the format given by `crate::constants::SRC_DATE_FMT`.
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
    /// Link to the app where this code is deployed
    pub app_url: &'a str,
    /// Link to the repo where this code is hosted
    pub repo_url: &'a str,
}

/// The template for a 404 not found page
#[derive(Template, Debug)]
#[template(path = "not_found.html")]
pub struct NotFoundTemplate<'a> {
    /// The date of the requested comic, if available
    pub date: Option<&'a str>,
    /// Link to the repo where this code is hosted
    pub repo_url: &'a str,
}

/// The template for a 500 internal server error page
#[derive(Template, Debug)]
#[template(path = "error.html")]
pub struct ErrorTemplate<'a> {
    /// The error message of the interval server error
    pub error: &'a str,
    /// Link to the repo where this code is hosted
    pub repo_url: &'a str,
}
