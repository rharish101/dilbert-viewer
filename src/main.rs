// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The main file for running the viewer app
use std::io::stdout;

use clap::{Parser, Subcommand};
use jiff::civil::Date;
use portpicker::{is_free, pick_unused_port};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::filter::EnvFilter;

/// Default port when one isn't specified
const PORT: u16 = 5000;

/// Parse a date given in the `YYYY-MM-DD` format from the CLI.
fn parse_date(date_str: &str) -> Result<Date, String> {
    date_str.parse::<Date>().map_err(|err| err.to_string())
}

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[arg(short, long, env = "DATABASE_URL")]
    database_url: String,

    // Use the `RUST_LOG` env var, like `env_logger`, but with a default.
    #[arg(short, long, env = "RUST_LOG", default_value = "warn")]
    log_level: String,

    #[command(subcommand)]
    command: Command,
}

/// The subcommands of the viewer app.
#[derive(Subcommand)]
enum Command {
    /// Run the web viewer, serving comics from the database
    Serve {
        #[arg(short, long, default_value = "localhost")]
        host: String,

        #[arg(short, long, env = "PORT", default_value_t = choose_port())]
        port: u16,
    },

    /// Scrape the Wayback Machine and write comics into the database
    Populate {
        /// Scrape only these dates (YYYY-MM-DD); the default is to scrape all dates
        #[arg(value_parser = parse_date)]
        date: Vec<Date>,

        /// Re-scrape and overwrite dates that already exist in the database
        #[arg(short = 'f', long)]
        overwrite: bool,
    },
}

/// Initialize the logger from the `RUST_LOG` environment variable, with a default.
fn init_logger(log_level: &str) -> WorkerGuard {
    // Log to stdout in a non-blocking way using a logging thread.
    let (writer, guard) = tracing_appender::non_blocking(stdout());

    let builder = EnvFilter::builder();
    let filter = match builder.parse(log_level) {
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
    if is_free(PORT) {
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

    // The non-blocking writer stays active as long as `_guard` is not dropped.
    let _guard = init_logger(&cli.log_level);

    match cli.command {
        Command::Serve { host, port } => {
            dilbert_viewer::serve(format!("{host}:{port}"), cli.database_url, None).await
        }
        Command::Populate { date, overwrite } => {
            dilbert_viewer::populate(&cli.database_url, date, overwrite, None, None).await
        }
    }
}
