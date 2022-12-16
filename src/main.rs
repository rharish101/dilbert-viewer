//! The main file for running the viewer app
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
use std::io::stdout;
use std::str::FromStr;

use portpicker::{is_free, pick_unused_port};
use tracing::error;
use tracing_subscriber::EnvFilter;

/// Default port when one isn't specified
// This is Heroku's default port when running locally
const PORT: u16 = 5000;

// Environment variables that are read
/// Port on which to run the server
const PORT_VAR: &str = "PORT";
/// Redis database connection URL
const REDIS_URL_VAR: &str = "REDIS_TLS_URL";

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Log to stdout in a non-blocking way using a logging thread.
    let (writer, _guard) = tracing_appender::non_blocking(stdout());
    tracing_subscriber::fmt()
        // Use the `RUST_LOG` env var, like `env_logger`.
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(writer)
        .init();

    let port = if let Some(port) = env::var(PORT_VAR)
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
    let host = format!("0.0.0.0:{port}");

    let db_url = if let Ok(db_url) = env::var(REDIS_URL_VAR) {
        Some(db_url)
    } else {
        error!("Missing environment variable for the database URL: {REDIS_URL_VAR}");
        None
    };

    dilbert_viewer::run(host, db_url, None, None).await
}
