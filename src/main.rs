// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The main file for running the viewer app
use std::env;
use std::io::stdout;
use std::str::FromStr;

use portpicker::{is_free, pick_unused_port};
use tracing::error;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::filter::{EnvFilter, LevelFilter};

/// Default port when one isn't specified
// This is Heroku's default port when running locally
const PORT: u16 = 5000;

/// Default log level
const LOG_LEVEL: LevelFilter = LevelFilter::WARN;

// Environment variables that are read
/// Port on which to run the server
const PORT_VAR: &str = "PORT";
/// Log level
const LOG_VAR: &str = "RUST_LOG";
/// Redis database connection URL
const REDIS_URL_VAR: &str = "REDIS_TLS_URL";

/// Initialize the logger from the `RUST_LOG` environment variable, with a default.
fn init_logger() -> WorkerGuard {
    // Log to stdout in a non-blocking way using a logging thread.
    let (writer, guard) = tracing_appender::non_blocking(stdout());

    // Use the `RUST_LOG` env var, like `env_logger`, but with a default.
    let builder = EnvFilter::builder().with_default_directive(LOG_LEVEL.into());
    let filter = match builder.parse(env::var(LOG_VAR).unwrap_or_default()) {
        Ok(filter) => filter,
        Err(err) => {
            println!("Invalid log level: {err}");
            builder.parse_lossy("")
        }
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .init();

    guard
}

/// Choose the port from an environment variable, with a fallback.
fn choose_port() -> u16 {
    if let Some(port) = env::var(PORT_VAR)
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
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // The non-blocking writer stays active as long as `_guard` is not dropped.
    let _guard = init_logger();

    let host = format!("0.0.0.0:{}", choose_port());

    let db_url = if let Ok(db_url) = env::var(REDIS_URL_VAR) {
        Some(db_url)
    } else {
        error!("Missing environment variable for the database URL: {REDIS_URL_VAR}");
        None
    };

    dilbert_viewer::run(host, db_url, None, None, None).await
}
