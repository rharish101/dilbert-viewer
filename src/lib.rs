// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The public interface for running the viewer app
//!
//! This file is separated from `main.rs` for the sole purpose of integration testing.
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
    middleware::{Compress, DefaultHeaders, Logger},
    web, App, Error as WebError, HttpServer,
};
use tracing::{error, info};

use crate::app::{serve_404, Viewer};
use crate::constants::{CSP, SRC_BASE_URL, STATIC_DIR, STATIC_URL};
use crate::db::get_db_pool;
use crate::handlers::{comic_page, latest_comic, minify_css, random_comic};
use crate::logging::TracingWrapper;

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
        } else {
            error!("Couldn't convert 404 page into UTF-8");
        }
    } else {
        error!("Couldn't render 404 page into bytes");
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
            .wrap(Logger::new(
                "ip=%{r}a req_line=\"%r\" referer=\"%{Referer}i\" user_agent=\"%{User-Agent}i\" \
                status=%s size=%bB time=%Ts",
            ))
            .wrap(TracingWrapper::default())
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

    info!("Starting server at {host}");
    server.bind(host)?.run().await
}
