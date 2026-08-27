// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Route handlers for the server
//!
//! This is kept separate from `lib.rs`, since actix-web handlers are pub by default.
use std::path::Path;

use actix_web::{HttpResponse, Responder, get, http::header::LOCATION, web};
use jiff::{Span, civil::Date};
use rand::{RngExt, rng};
use tracing::info;

use crate::app::{Viewer, serve_404, serve_css, serve_js};
use crate::constants::{FIRST_COMIC, LAST_COMIC, SRC_DATE_FMT, STATIC_DIR};
use crate::datetime::str_to_date;

/// Serve the last comic.
#[get("/")]
async fn last_comic(viewer: web::Data<Viewer>) -> impl Responder {
    // If there is no comic for this date yet, "dilbert.com" will redirect to the homepage. The
    // code can handle this by instead showing the contents of the last comic.
    let last = str_to_date(LAST_COMIC, SRC_DATE_FMT)
        .expect("Variable LAST_COMIC not in format of variable SRC_DATE_FMT");
    viewer.serve_comic(&last).await
}

/// Serve the comic requested in the given URL.
#[get("/{year}-{month}-{day}")]
async fn comic_page(viewer: web::Data<Viewer>, path: web::Path<(i16, u8, u8)>) -> impl Responder {
    let (year, month, day) = path.into_inner();

    // Check to see if the date is invalid.
    if let Ok(date) = Date::new(year, month as i8, day as i8) {
        viewer.serve_comic(&date).await
    } else {
        // Note: Numbers too large to fit in Jiff's native types (e.g. a 5-digit year) are rejected
        // by actix as 400s before this is even called.
        info!("Invalid date requested: ({year}-{month}-{day})");
        serve_404(None)
    }
}

/// Serve a random comic.
#[get("/random")]
async fn random_comic() -> impl Responder {
    let first = str_to_date(FIRST_COMIC, SRC_DATE_FMT)
        .expect("Variable FIRST_COMIC not in format of variable SRC_DATE_FMT");
    let last = str_to_date(LAST_COMIC, SRC_DATE_FMT)
        .expect("Variable LAST_COMIC not in format of variable SRC_DATE_FMT");

    let mut rng = rng();
    // Offset (in days) from the first date
    let rand_offset = rng.random_range(0..(last - first).get_days());
    let rand_date = first + Span::new().days(rand_offset);
    info!("Chose random comic date: {rand_date}");

    let location = format!("/{}", rand_date.strftime(SRC_DATE_FMT));
    HttpResponse::TemporaryRedirect()
        .append_header((LOCATION, location))
        .finish()
}

/// Serve CSS after minification.
#[get("/{path}.css")]
async fn minify_css(path: web::Path<String>) -> impl Responder {
    let stem = path.into_inner();
    let css_path = Path::new(STATIC_DIR).join(stem + ".css");
    serve_css(&css_path).await
}

/// Serve JS after minification.
#[get("/{path}.js")]
async fn minify_js(path: web::Path<String>) -> impl Responder {
    let stem = path.into_inner();
    let js_path = Path::new(STATIC_DIR).join(stem + ".js");
    serve_js(&js_path).await
}
