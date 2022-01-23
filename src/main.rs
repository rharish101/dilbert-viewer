//! The main file for the viewer app
mod app;
mod constants;
mod errors;
mod scrapers;
mod templates;
mod utils;

use std::env;
use std::io::Result as IOResult;
use std::str::FromStr;

use actix_files::Files;
use actix_web::{
    dev::{ServiceRequest, ServiceResponse},
    get, web, App, Error as WebError, HttpResponse, HttpServer, Responder,
};
use chrono::Duration as DateDuration;
use log::info;
use rand::{thread_rng, Rng};

use crate::app::Viewer;
use crate::constants::{DATE_FMT, FIRST_COMIC, STATIC_URL};
use crate::utils::{curr_date, str_to_date};

/// Serve the latest comic
#[get("/")]
async fn latest_comic(viewer: web::Data<Viewer>) -> impl Responder {
    // If there is no comic for this date yet, "dilbert.com" will redirect to the homepage. The
    // code can handle this by instead showing the contents of the latest comic.
    let today = curr_date().format(DATE_FMT).to_string();

    // If there is no comic for this date yet, we don't want to raise a 404, so just show the exact
    // latest date without a redirection (to preserve the URL and load faster)
    viewer.serve_comic(&today, true).await
}

/// Serve the comic requested in the given URL
#[get("/{year}-{month}-{day}")]
async fn comic_page(viewer: web::Data<Viewer>, path: web::Path<(u16, u16, u16)>) -> impl Responder {
    let (year, month, day) = path.into_inner();

    // NOTE: This depends on the format given by `crate::constants::DATE_FMT`
    let date = format!("{:04}-{:02}-{:02}", year, month, day);

    // Check to see if the date is invalid
    if str_to_date(&date, DATE_FMT).is_err() {
        Viewer::serve_404(None)
    } else {
        viewer.serve_comic(&date, false).await
    }
}

/// Serve a random comic
#[get("/random")]
async fn random_comic() -> impl Responder {
    let first = str_to_date(FIRST_COMIC, DATE_FMT).unwrap();
    // There might not be any comic for this date yet, so exclude the latest date
    let latest = curr_date() - DateDuration::days(1);

    let mut rng = thread_rng();
    // Offset (in days) from the first date
    let rand_offset = rng.gen_range(0..(latest - first).num_days());
    let rand_date = first + DateDuration::days(rand_offset);

    let location = format!("/{}", rand_date.format(DATE_FMT));
    HttpResponse::TemporaryRedirect()
        .append_header(("Location", location))
        .finish()
}

/// Handles invalid URLs by sending 404s
///
/// This is to be invoked when the actix static file service doesn't find a file
async fn invalid_url(req: ServiceRequest) -> Result<ServiceResponse, WebError> {
    let (http_req, _payload) = req.into_parts();
    Ok(ServiceResponse::new(http_req, Viewer::serve_404(None)))
}

#[actix_web::main]
async fn main() -> IOResult<()> {
    pretty_env_logger::init();

    let viewer = web::Data::new(Viewer::new().await.unwrap());
    let host = format!("0.0.0.0:{}", env::var("PORT").unwrap());
    info!("Starting server at {}", host);

    let mut server = HttpServer::new(move || {
        // Needs to be different for every worker, so invoke it here instead of outside
        let static_service =
            Files::new(STATIC_URL, String::from("static")).default_handler(invalid_url);
        App::new()
            .app_data(viewer.clone())
            .service(latest_comic)
            .service(comic_page)
            .service(random_comic)
            // This should be at the end, otherwise everything after this will be ignored
            .service(static_service)
    });

    // Currently the Rust buildpack for Heroku doesn't support WEB_CONCURRENCY, so only use it if
    // present
    if let Ok(web_concurrency) = env::var("WEB_CONCURRENCY") {
        let num_workers = usize::from_str(web_concurrency.as_str()).unwrap();
        server = server.workers(num_workers);
    }

    server.bind(host)?.run().await
}
