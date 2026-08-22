// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The public interface for running the viewer app
//!
//! This file is separated from `main.rs` for the sole purpose of integration testing.
mod app;
mod constants;
mod datetime;
mod db;
mod entities;
mod errors;
mod handlers;
mod logging;
mod scraper;
mod templates;

use actix_files::Files;
use actix_web::{
    App, Error as WebError, HttpServer,
    body::MessageBody,
    dev::{ServiceRequest, ServiceResponse},
    middleware::{Compress, DefaultHeaders, Logger},
    web,
};
use chrono::NaiveDate;
use tracing::{error, info};

use crate::app::{Viewer, serve_404};
use crate::constants::{ARC_BASE_URL, CDX_URL, CSP, STATIC_DIR, STATIC_URL};
use crate::db::{ensure_schema, init_db};
use crate::handlers::{comic_page, last_comic, minify_css, minify_js, random_comic};
use crate::logging::TracingWrapper;
use crate::scraper::ComicScraper;

#[cfg(feature = "test-support")]
/// Test-only helpers for the integration tests
pub mod test {
    use chrono::NaiveDate;
    use sea_orm::{DatabaseConnection, DbErr};

    use crate::db::{ensure_schema, init_db, insert_comic};
    use crate::scraper::ComicData;

    /// Create a database at the given URL, sync the schema, and insert a
    /// placeholder comic for each of the given dates.
    ///
    /// # Arguments
    /// * `url` - The URL to connect to the database with
    /// * `dates` - The dates for which a placeholder comic is to be inserted
    pub async fn seed_db(url: &str, dates: &[NaiveDate]) -> Result<DatabaseConnection, DbErr> {
        let db = init_db(url).await?;
        ensure_schema(&db).await?;
        for &date in dates {
            let comic_data = ComicData {
                title: String::from("Test comic"),
                img_url: format!(
                    "https://web.archive.org/web/2000/https://dilbert.com/strip/{date}"
                ),
                img_width: 580,
                img_height: 140,
                permalink: format!("https://dilbert.com/strip/{date}"),
            };
            insert_comic(&db, date, comic_data, false).await?;
        }
        Ok(db)
    }
}

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

/// Run the comics server.
///
/// # Arguments
/// * `host` - The host and port where to start the server
/// * `db_url` - The URL to the database
/// * `workers` - The optional number of workers to use
pub async fn serve(host: String, db_url: String, workers: Option<usize>) -> std::io::Result<()> {
    // Create all worker-shared (i.e. thread-safe) structs here
    let db = init_db(&db_url)
        .await
        .expect("Couldn't connect to the database");
    if let Err(err) = ensure_schema(&db).await {
        error!("Couldn't sync the database schema: {err}.");
    };

    let mut server = HttpServer::new(move || {
        // Create all worker-specific (i.e. thread-unsafe) structs here
        let viewer = Viewer::new(db.clone());
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
            .wrap(TracingWrapper)
            .service(last_comic)
            .service(comic_page)
            .service(random_comic)
            .service(minify_css)
            .service(minify_js)
            // This should be at the end, otherwise everything after this will be ignored.
            .service(static_service)
    });

    if let Some(workers) = workers {
        server = server.workers(workers);
    };

    info!("Starting server at {host}");
    server.bind(&host)?.run().await
}

/// Populate the database by scraping info.
///
/// # Arguments
/// * `db_url` - The URL to the database
/// * `dates` - If non-empty, only populate these dates; otherwise, populate every date from the
///   first comic until today
/// * `overwrite` - Whether to re-scrape and overwrite dates that already exist
/// * `source_url` - The optional URL to the custom comic source
/// * `cdx_url` - The optional URL to the custom CDX API
pub async fn populate(
    db_url: &str,
    dates: Vec<NaiveDate>,
    overwrite: bool,
    source_url: Option<String>,
    cdx_url: Option<String>,
) -> std::io::Result<()> {
    let db = init_db(db_url)
        .await
        .expect("Couldn't connect to the database");
    if let Err(err) = ensure_schema(&db).await {
        error!("Couldn't sync the database schema: {err}.");
    };

    let scraper = ComicScraper::new(
        db,
        source_url.unwrap_or_else(|| String::from(ARC_BASE_URL)),
        cdx_url.unwrap_or_else(|| String::from(CDX_URL)),
    );
    let summary = scraper.scrape_comic_data_multi(dates, overwrite).await;
    info!(
        "Done: {} dates processed ({} skipped, {} empty, {} populated, {} failed)",
        summary.total, summary.skipped, summary.empty, summary.populated, summary.failed,
    );
    Ok(())
}
