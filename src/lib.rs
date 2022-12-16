//! The public interface for running the viewer app
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
mod datetime;
mod db;
mod errors;
mod handlers;
mod logging;
mod scrapers;
mod templates;

use actix_files::Files;
use actix_web::{
    body::MessageBody,
    dev::{ServiceRequest, ServiceResponse},
    middleware::{Compress, DefaultHeaders},
    web, App, Error as WebError, HttpServer,
};
use tracing::error;
use tracing_actix_web::TracingLogger;

use crate::app::{serve_404, Viewer};
use crate::constants::{CSP, SRC_BASE_URL, STATIC_DIR, STATIC_URL};
use crate::db::get_db_pool;
use crate::handlers::{comic_page, latest_comic, minify_css, random_comic};
use crate::logging::RequestSpanBuilder;

/// Handle invalid URLs by sending 404s.
///
/// This is to be invoked when the actix static file service doesn't find a file.
async fn invalid_url(req: ServiceRequest) -> Result<ServiceResponse, WebError> {
    let (http_req, _payload) = req.into_parts();
    Ok(ServiceResponse::new(http_req, serve_404(None)))
}

/// Get the static file handling service.
fn get_static_service() -> Files {
    let mut service = Files::new(STATIC_URL, String::from(STATIC_DIR)).default_handler(invalid_url);
    if let Ok(bytes) = serve_404(None).into_body().try_into_bytes() {
        if let Ok(html) = std::str::from_utf8(&bytes) {
            service = service.index_file(html);
        }
    }
    service
}

/// Run the server.
///
/// # Arguments
/// * `host` - The host and port where to start the server
/// * `db_url` - The optional URL to the database
/// * `source_url` - The optional URL to the custom comic source
/// * `workers` - The optional number of workers to use
pub async fn run(
    host: String,
    db_url: Option<String>,
    source_url: Option<String>,
    workers: Option<usize>,
) -> std::io::Result<()> {
    // Create all worker-shared (i.e. thread-safe) structs here
    let db_pool = if let Some(db_url) = db_url {
        match get_db_pool(db_url) {
            Ok(pool) => Some(pool),
            Err(err) => {
                error!("Couldn't create DB pool: {err}. No caching will be available.",);
                None
            }
        }
    } else {
        error!("No DB URL given. No caching will be available.");
        None
    };

    let mut server = HttpServer::new(move || {
        // Create all worker-specific (i.e. thread-unsafe) structs here
        let viewer = Viewer::new(
            db_pool.clone(),
            source_url.clone().unwrap_or_else(|| SRC_BASE_URL.into()),
        );
        let static_service = get_static_service();
        Files::new(STATIC_URL, String::from(STATIC_DIR)).default_handler(invalid_url);
        let default_headers = DefaultHeaders::new().add(("Content-Security-Policy", CSP));

        App::new()
            .app_data(web::Data::new(viewer))
            .wrap(Compress::default())
            .wrap(default_headers)
            .wrap(TracingLogger::<RequestSpanBuilder>::new())
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
