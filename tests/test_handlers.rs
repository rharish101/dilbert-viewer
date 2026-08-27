// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::Duration;

use actix_web::rt::spawn;
use dilbert_viewer::{serve, test};
use jiff::civil::Date;
use portpicker::pick_unused_port;
use reqwest::header::{CONTENT_TYPE, LOCATION};
use reqwest::{Client, Response, StatusCode, redirect::Policy};
use sea_orm::DatabaseConnection;
use std::io;
use test_case::test_case;
use tokio::task::JoinHandle;

/// Hostname where to start the server
const HOST: &str = "localhost";
/// Timeout (in seconds) for getting a response from the server
const RESP_TIMEOUT: u64 = 5;
/// Date of the first ever Dilbert comic
const FIRST_COMIC: &str = "1989-04-16";
/// Date of the last available Dilbert comic
const LAST_COMIC: &str = "2023-03-12";
/// Number of times to run the random comic test
const RAND_TEST_ITER: usize = 10;
/// Number of times to retry for transient failures
const GET_FAIL_LIMIT: u32 = 50;

/// Get the HTTP client.
fn get_http_client() -> Client {
    let timeout = Duration::from_secs(RESP_TIMEOUT);
    Client::builder()
        .redirect(Policy::none())
        .timeout(timeout)
        .build()
        .expect("Couldn't build the HTTP client")
}

/// Test if an HTTP response is a valid HTML page.
///
/// # Arguments
/// * `resp` - The HTTP response
/// * `expected` - The expected Content-Type header
fn test_content_type(resp: &Response, expected: &str) {
    // Check the "Content-Type" header.
    let content_type = resp
        .headers()
        .get(CONTENT_TYPE)
        .expect("Missing Content-Type header")
        .to_str()
        .expect("Content-Type header is not ASCII");
    assert!(
        content_type.contains(expected),
        "Wrong response content type"
    );
}

/// Create a named shared-cache in-memory SQLite database, sync the schema, and populate it
/// with the given comics.
///
/// A *named* shared-cache DB (`file:<name>?mode=memory&cache=shared`) is used instead of
/// plain `:memory:` because `serve()`'s connection pool opens multiple connections, and each
/// connection to an unnamed `:memory:` database would get its own separate (empty) database.
///
/// Note that a shared-cache in-memory database only exists while at least one connection to it
/// is open, so the returned connection must be kept alive for the duration of the test.
///
/// # Arguments
/// * `port` - A port unique to this test, used to make the database name unique
/// * `comics` - The dates for which a comic is to be inserted
///
/// # Returns
/// * The URL to connect to the database with
/// * The connection that keeps the database alive
async fn make_db(port: u16, comics: &[&str]) -> (String, DatabaseConnection) {
    // The `file:` in the database name is percent-encoded, since the URL is first validated
    // as a generic URL (where `file:` in the host position is invalid) before SQLx parses it.
    let db_url = format!("sqlite://file%3Adilbert-{port}?mode=memory&cache=shared");
    let dates: Vec<Date> = comics
        .iter()
        .map(|s| s.parse::<Date>().expect("Invalid test date"))
        .collect();
    let db = test::seed_db(&db_url, &dates)
        .await
        .expect("Couldn't seed the test database");
    (db_url, db)
}

/// Start the server on the given port, backed by a database containing the given comics.
///
/// # Arguments
/// * `port` - The port to start the server on
/// * `comics` - The dates for which a comic is to be pre-populated in the database
///
/// # Returns
/// * The spawned server task
/// * The HTTP client
/// * The connection that keeps the (in-memory) database alive
async fn start_server(
    port: u16,
    comics: &[&str],
) -> (JoinHandle<io::Result<()>>, Client, DatabaseConnection) {
    let (db_url, db) = make_db(port, comics).await;
    let host = format!("{HOST}:{port}");

    // Start the server on a single thread.
    let handle = spawn(serve(host, db_url, Some(1)));
    (handle, get_http_client(), db)
}

/// Send a GET request, retrying on transient failures while the server is starting up.
///
/// # Arguments
/// * `client` - The HTTP client
/// * `url` - The URL to request
async fn send_get(client: &Client, url: &str) -> Response {
    let mut last_err = None;
    for _ in 0..GET_FAIL_LIMIT {
        match client.get(url).send().await {
            Ok(resp) => return resp,
            // The server may not be listening yet.
            Err(err) => {
                last_err = Some(err);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
    panic!("Failed to send request to server: {}", last_err.unwrap());
}

#[actix_web::test]
/// Test whether the homepage gives the last comic.
async fn test_last_comic() {
    let port = pick_unused_port().expect("Couldn't find an available port");
    let (handle, client, _db) = start_server(port, &[LAST_COMIC]).await;
    let resp = send_get(&client, &format!("http://{HOST}:{port}/")).await;

    // Close the server.
    handle.abort();

    assert_eq!(resp.status(), StatusCode::OK, "Response status is not OK");
    test_content_type(&resp, "text/html");
}

#[test_case(2000, 1, 1, true; "existing comic")]
#[test_case(2000, 1, 2, false; "valid date without a comic")]
#[test_case(2000, 0, 0, false; "invalid date")]
#[actix_web::test]
/// Test a comic webpage.
///
/// # Arguments
/// * `year` - The year of the comic
/// * `month` - The month of the comic
/// * `day` - The day of the comic
/// * `has_comic` - Whether the comic for the given (valid) date is in the database
async fn test_comic(year: i16, month: u8, day: u8, has_comic: bool) {
    let port = pick_unused_port().expect("Couldn't find an available port");

    let date_str = format!("{year:04}-{month:02}-{day:02}");
    // Mirrors the validation in the `comic_page` handler.
    let expected_status = if Date::new(year, month as i8, day as i8).is_ok() && has_comic {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    };
    let comics = if expected_status == StatusCode::OK {
        vec![date_str.as_str()]
    } else {
        vec![]
    };

    let (handle, client, _db) = start_server(port, &comics).await;
    let resp = send_get(&client, &format!("http://{HOST}:{port}/{date_str}")).await;

    // Close the server.
    handle.abort();

    assert_eq!(resp.status(), expected_status, "Unexpected response status");
    if expected_status == StatusCode::OK {
        test_content_type(&resp, "text/html");
    }
}

#[actix_web::test]
/// Test the random comic request.
async fn test_random_comic() {
    let port = pick_unused_port().expect("Couldn't find an available port");
    let (handle, client, _db) = start_server(port, &[]).await;

    let first_comic = FIRST_COMIC.parse::<Date>().unwrap();
    let last_comic = LAST_COMIC.parse::<Date>().unwrap();

    for _ in 0..RAND_TEST_ITER {
        let resp = send_get(&client, &format!("http://{HOST}:{port}/random")).await;
        assert_eq!(
            resp.status(),
            StatusCode::TEMPORARY_REDIRECT,
            "Response status is not a temporary redirect",
        );

        // Check that the comic it redirects to is valid.
        let location = resp
            .headers()
            .get(LOCATION)
            .expect("Missing Location header")
            .to_str()
            .expect("Location header is not ASCII");
        let random_date = location[1..]
            .parse::<Date>()
            .expect("Redirected to invalid date");
        assert!(
            random_date >= first_comic && random_date <= last_comic,
            "Redirected to invalid date"
        );
    }

    // Close the server.
    handle.abort();
}

#[test_case("styles.css", StatusCode::OK, "text/css"; "css")]
#[test_case("script.js", StatusCode::OK, "text/javascript"; "js")]
#[test_case("robots.txt", StatusCode::OK, "text/plain"; "misc")]
#[test_case("foo", StatusCode::NOT_FOUND, "text/html"; "non-existant")]
#[test_case("//", StatusCode::NOT_FOUND, "text/html"; "existing directory")]
#[actix_web::test]
/// Test the static file service.
///
/// # Arguments
/// * `path` - The URL path to the static file
/// * `status_code` - The expected HTTP status code
/// * `content_type` - The expected Content-Type header
async fn test_static(path: &str, status_code: StatusCode, content_type: &str) {
    let port = pick_unused_port().expect("Couldn't find an available port");
    let (handle, client, _db) = start_server(port, &[]).await;
    let resp = send_get(&client, &format!("http://{HOST}:{port}/{path}")).await;

    // Close the server.
    handle.abort();

    assert_eq!(resp.status(), status_code, "Unexpected response status");
    test_content_type(&resp, content_type);
}
