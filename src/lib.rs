//! Contains the server and its responders.
//!
//! This file is separated from `main.rs` for the sole purpose of integration testing.
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
mod client;
mod constants;
mod db;
mod errors;
mod handlers;
mod scrapers;
mod templates;
mod utils;

use actix_files::Files;
use actix_web::{
    dev::{ServiceRequest, ServiceResponse},
    middleware::{Compress, DefaultHeaders, Logger},
    web, App, Error as WebError, HttpServer,
};
use log::error;

use crate::app::{serve_404, Viewer};
use crate::constants::{CSP, SRC_BASE_URL, STATIC_DIR, STATIC_URL};
use crate::db::get_db_pool;
use crate::handlers::{comic_page, latest_comic, minify_css, random_comic};

/// Handle invalid URLs by sending 404s.
///
/// This is to be invoked when the actix static file service doesn't find a file.
async fn invalid_url(req: ServiceRequest) -> Result<ServiceResponse, WebError> {
    let (http_req, _payload) = req.into_parts();
    Ok(ServiceResponse::new(http_req, serve_404(None)))
}

/// Run the server.
///
/// # Arguments
/// * `host` - The host and port where to start the server
/// * `source_url` - The optional URL to the custom comic source
/// * `workers` - The optional number of workers to use
pub async fn run(
    host: String,
    source_url: Option<String>,
    workers: Option<usize>,
) -> std::io::Result<()> {
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
        let viewer = Viewer::new(
            db_pool.clone(),
            source_url.clone().unwrap_or_else(|| SRC_BASE_URL.into()),
        );
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

    if let Some(workers) = workers {
        server = server.workers(workers);
    };

    server.bind(host)?.run().await
}
