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
use std::env;
use std::str::FromStr;

use log::error;
use portpicker::{is_free, pick_unused_port};

/// Default port when one isn't specified
// This is Heroku's default port when running locally
pub const PORT: u16 = 5000;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    pretty_env_logger::init();

    let port = if let Some(port) = env::var("PORT")
        .ok()
        .and_then(|port| u16::from_str(&port).ok())
    {
        port
    } else if is_free(PORT) {
        PORT
    } else if let Some(port) = pick_unused_port() {
        port
    } else {
        panic!("Couldn't find any unused TCP port")
    };
    let host = format!("0.0.0.0:{}", port);
    println!("Starting server at {}", host);

    let db_url = if let Ok(db_url) = env::var("REDIS_TLS_URL") {
        Some(db_url)
    } else {
        error!("Missing environment variable for the database URL");
        None
    };

    // Currently the Rust buildpack for Heroku doesn't support WEB_CONCURRENCY, so only use it if
    // present.
    let workers = env::var("WEB_CONCURRENCY")
        .ok()
        .and_then(|workers| usize::from_str(&workers).ok());

    dilbert_viewer::run(host, db_url, None, workers).await
}
