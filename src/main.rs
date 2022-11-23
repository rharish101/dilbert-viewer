//! The main file for the viewer app
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
mod app;
mod constants;
mod errors;
mod scrapers;
mod templates;
mod utils;

use std::env;
use std::io::Result as IOResult;
use std::path::Path;
use std::str::FromStr;
use std::time::Duration as TimeDuration;

use actix_files::Files;
use actix_web::{
    dev::{ServiceRequest, ServiceResponse},
    get,
    middleware::{Compress, DefaultHeaders, Logger},
    web, App, Error as WebError, HttpResponse, HttpServer, Responder,
};
use chrono::{Duration as DateDuration, NaiveDate};
use deadpool_redis::{Config as RedisConfig, Pool as RedisPool, Runtime};
use log::{error, info};
use rand::{thread_rng, Rng};

use crate::app::Viewer;
use crate::constants::{
    CSP, DB_TIMEOUT, FIRST_COMIC, MAX_DB_CONN, PORT, SRC_DATE_FMT, STATIC_DIR, STATIC_URL,
};
use crate::errors::DbInitError;
use crate::utils::{curr_date, str_to_date};

/// Initialize the database connection pool for caching data.
async fn get_db_pool() -> Result<RedisPool, DbInitError> {
    // Heroku needs SSL for its Redis addon, but uses a self-signed certificate. So simply disable
    // verification while keeping SSL.
    let config = RedisConfig::from_url(env::var("REDIS_TLS_URL")? + "#insecure");
    let pool_builder = config
        .builder()?
        .runtime(Runtime::Tokio1)
        .max_size(MAX_DB_CONN)
        .wait_timeout(Some(TimeDuration::from_secs(DB_TIMEOUT)));
    Ok(pool_builder.build()?)
}

/// Serve the latest comic.
#[get("/")]
async fn latest_comic(viewer: web::Data<Viewer>) -> impl Responder {
    // If there is no comic for this date yet, "dilbert.com" will redirect to the homepage. The
    // code can handle this by instead showing the contents of the latest comic.
    let today = curr_date();

    // If there is no comic for this date yet, we don't want to raise a 404, so just show the exact
    // latest date without a redirection (to preserve the URL and load faster).
    viewer.serve_comic(today, true).await
}

/// Serve the comic requested in the given URL.
#[get("/{year}-{month}-{day}")]
async fn comic_page(viewer: web::Data<Viewer>, path: web::Path<(i32, u32, u32)>) -> impl Responder {
    let (year, month, day) = path.into_inner();

    // Check to see if the date is invalid.
    if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
        viewer.serve_comic(date, false).await
    } else {
        Viewer::serve_404(None)
    }
}

/// Serve a random comic.
#[get("/random")]
async fn random_comic() -> impl Responder {
    let first = str_to_date(FIRST_COMIC, SRC_DATE_FMT)
        .expect("Variable FIRST_COMIC not in format of variable SRC_DATE_FMT");
    // There might not be any comic for this date yet, so exclude the latest date.
    let latest = curr_date() - DateDuration::days(1);

    let mut rng = thread_rng();
    // Offset (in days) from the first date
    let rand_offset = rng.gen_range(0..(latest - first).num_days());
    let rand_date = first + DateDuration::days(rand_offset);

    let location = format!("/{}", rand_date.format(SRC_DATE_FMT));
    HttpResponse::TemporaryRedirect()
        .append_header(("Location", location))
        .finish()
}

/// Handle invalid URLs by sending 404s.
///
/// This is to be invoked when the actix static file service doesn't find a file.
async fn invalid_url(req: ServiceRequest) -> Result<ServiceResponse, WebError> {
    let (http_req, _payload) = req.into_parts();
    Ok(ServiceResponse::new(http_req, Viewer::serve_404(None)))
}

/// Serve CSS after minification
#[get("/{path}.css")]
async fn minify_css(path: web::Path<String>) -> impl Responder {
    let stem = path.into_inner();
    let css_path = Path::new(STATIC_DIR).join(stem + ".css");
    Viewer::serve_css(&css_path).await
}

#[actix_web::main]
async fn main() -> IOResult<()> {
    pretty_env_logger::init();

    let host = format!(
        "0.0.0.0:{}",
        env::var("PORT").unwrap_or_else(|_| String::from(PORT))
    );
    info!("Starting server at {}", host);

    // Create all worker-shared (i.e. thread-safe) structs here
    let db_pool = match get_db_pool().await {
        Ok(pool) => Some(pool),
        Err(err) => {
            error!(
                "Couldn't create DB pool: {}. No caching will be available.",
                err
            );
            None
        }
    };

    let mut server = HttpServer::new(move || {
        // Create all worker-specific (i.e. thread-unsafe) structs here
        let viewer = Viewer::new(db_pool.clone());
        let static_service =
            Files::new(STATIC_URL, String::from(STATIC_DIR)).default_handler(invalid_url);
        let default_headers = DefaultHeaders::new().add(("Content-Security-Policy", CSP));

        App::new()
            .app_data(web::Data::new(viewer))
            .wrap(Compress::default())
            .wrap(default_headers)
            .wrap(Logger::default())
            .service(latest_comic)
            .service(comic_page)
            .service(random_comic)
            .service(minify_css)
            // This should be at the end, otherwise everything after this will be ignored.
            .service(static_service)
    });

    // Currently the Rust buildpack for Heroku doesn't support WEB_CONCURRENCY, so only use it if
    // present.
    if let Ok(web_concurrency) = env::var("WEB_CONCURRENCY") {
        if let Ok(num_workers) = usize::from_str(web_concurrency.as_str()) {
            server = server.workers(num_workers);
        }
    }

    server.bind(host)?.run().await
}
