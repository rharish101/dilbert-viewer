//! Route handlers for the server
//!
//! This is kept separate from `lib.rs`, since actix-web handlers are pub by default.
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
use std::path::Path;

use actix_web::{get, http::header::LOCATION, web, HttpResponse, Responder};
use chrono::{Duration, NaiveDate};
use deadpool_redis::Pool;
use rand::{thread_rng, Rng};

use crate::app::{serve_404, serve_css, Viewer};
use crate::constants::{FIRST_COMIC, SRC_DATE_FMT, STATIC_DIR};
use crate::utils::{curr_date, str_to_date};

/// Serve the latest comic.
#[get("/")]
async fn latest_comic(viewer: web::Data<Viewer<Pool>>) -> impl Responder {
    // If there is no comic for this date yet, "dilbert.com" will redirect to the homepage. The
    // code can handle this by instead showing the contents of the latest comic.
    let today = curr_date();

    // If there is no comic for this date yet, we don't want to raise a 404, so just show the exact
    // latest date without a redirection (to preserve the URL and load faster).
    viewer.serve_comic(today, true).await
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
        viewer.serve_comic(date, false).await
    } else {
        serve_404(None)
    }
}

/// Serve a random comic.
#[get("/random")]
async fn random_comic() -> impl Responder {
    let first = str_to_date(FIRST_COMIC, SRC_DATE_FMT)
        .expect("Variable FIRST_COMIC not in format of variable SRC_DATE_FMT");
    // There might not be any comic for this date yet, so exclude the latest date.
    let latest = curr_date() - Duration::days(1);

    let mut rng = thread_rng();
    // Offset (in days) from the first date
    let rand_offset = rng.gen_range(0..(latest - first).num_days());
    let rand_date = first + Duration::days(rand_offset);

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
