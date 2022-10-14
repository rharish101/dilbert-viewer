//! Contains structs for HTML templates
// This file is part of dilbert-viewer.
//
// Copyright (C) 2022  Harish Rajagopal <harish.rajagopals@gmail.com>
//
// dilbert-viewer is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// dilbert-viewer is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with dilbert-viewer.  If not, see <https://www.gnu.org/licenses/>.
use askama::Template;

use crate::scrapers::ComicData;

/// The main template for a comic
#[derive(Template)]
#[template(path = "comic.html")]
pub struct ComicTemplate<'a> {
    /// The scraped comic data
    pub data: &'a ComicData,

    // All date formats should conform to the format given by `crate::constants::DATE_FMT`.
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
pub struct NotFoundTemplate<'a> {
    /// The date of the requested comic, if available
    pub date: Option<&'a str>,
    /// Link to the repo where this code is hosted
    pub repo: &'a str,
}

/// The template for a 500 internal server error page
#[derive(Template)]
#[template(path = "error.html")]
pub struct ErrorTemplate<'a> {
    /// The error message of the interval server error
    pub error: &'a str,
    /// Link to the repo where this code is hosted
    pub repo: &'a str,
}
