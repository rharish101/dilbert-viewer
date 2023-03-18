// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Route handlers for the server
//!
//! This is kept separate from `lib.rs`, since actix-web handlers are pub by default.
use std::path::Path;

use actix_web::{get, http::header::LOCATION, web, HttpResponse, Responder};
use chrono::{Duration, NaiveDate};
use deadpool_redis::Pool;
use rand::{thread_rng, Rng};
use tracing::info;

use crate::app::{serve_404, serve_css, Viewer};
use crate::constants::{FIRST_COMIC, LAST_COMIC, SRC_DATE_FMT, STATIC_DIR};
use crate::datetime::str_to_date;

/// Serve the last comic.
#[get("/")]
async fn last_comic(viewer: web::Data<Viewer<Pool>>) -> impl Responder {
    // If there is no comic for this date yet, "dilbert.com" will redirect to the homepage. The
    // code can handle this by instead showing the contents of the last comic.
    let last = str_to_date(LAST_COMIC, SRC_DATE_FMT)
        .expect("Variable LAST_COMIC not in format of variable SRC_DATE_FMT");
    viewer.serve_comic(&last).await
}

/// Serve the comic requested in the given URL.
#[get("/{year}-{month}-{day}")]
async fn comic_page(
    viewer: web::Data<Viewer<Pool>>,
    path: web::Path<(i32, u32, u32)>,
) -> impl Responder {
    let (year, month, day) = path.into_inner();

    // Check to see if the date is invalid.
    if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
        viewer.serve_comic(&date).await
    } else {
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

    let mut rng = thread_rng();
    // Offset (in days) from the first date
    let rand_offset = rng.gen_range(0..(last - first).num_days());
    let rand_date = first + Duration::days(rand_offset);
    info!("Chose random comic date: {rand_date}");

    let location = format!("/{}", rand_date.format(SRC_DATE_FMT));
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
