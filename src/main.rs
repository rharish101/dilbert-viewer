// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The main file for running the viewer app
use std::env;
use std::io::stdout;
use std::str::FromStr;

use clap::{Parser, Subcommand};
use jiff::civil::Date;
use portpicker::{is_free, pick_unused_port};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::filter::{EnvFilter, LevelFilter};

/// Default port when one isn't specified
const PORT: u16 = 5000;

/// Default log level
const LOG_LEVEL: LevelFilter = LevelFilter::WARN;

// Environment variables that are read
/// Port on which to run the server
const PORT_VAR: &str = "PORT";
/// Log level
const LOG_VAR: &str = "RUST_LOG";
/// Database connection URL
const DATABASE_URL_VAR: &str = "DATABASE_URL";

/// Parse a date given in the `YYYY-MM-DD` format from the CLI.
fn parse_date(date_str: &str) -> Result<Date, String> {
    date_str.parse::<Date>().map_err(|err| err.to_string())
}

#[derive(Parser)]
#[command(
    version,
    name = env!("CARGO_PKG_NAME"),
    about = "Scrape and serve Dilbert comics from the Wayback Machine"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// The subcommands of the viewer app.
#[derive(Subcommand)]
enum Command {
    /// Run the web viewer, serving comics from the database
    Serve,
    /// Scrape the Wayback Machine and write comics into the database
    Populate {
        /// Scrape only these dates (YYYY-MM-DD); the default is to scrape all dates
        #[arg(value_parser = parse_date)]
        date: Vec<Date>,

        /// Re-scrape and overwrite dates that already exist in the database
        #[arg(long)]
        overwrite: bool,
    },
}

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

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let cli = Cli::parse();

    let db_url = env::var(DATABASE_URL_VAR).unwrap_or_else(|_| {
        format!("Missing environment variable for the database URL: {DATABASE_URL_VAR}")
    });

    // The non-blocking writer stays active as long as `_guard` is not dropped.
    let _guard = init_logger();

    match cli.command {
        Command::Serve => {
            let host = format!("0.0.0.0:{}", choose_port());
            dilbert_viewer::serve(host, db_url, None).await
        }
        Command::Populate { date, overwrite } => {
            dilbert_viewer::populate(&db_url, date, overwrite, None, None).await
        }
    }
}
