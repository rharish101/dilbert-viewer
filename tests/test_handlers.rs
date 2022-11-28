//! Tests for different route handlers of the web server.
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
use std::time::Duration;

use actix_web::rt::spawn;
use awc::{
    http::{header::CONTENT_TYPE, Method, StatusCode},
    Client, ClientResponse,
};
use chrono::{NaiveDate, Utc};
use dilbert_viewer::run;
use portpicker::pick_unused_port;
use test_case::test_case;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

/// Hostname where to start the server
const HOST: &str = "localhost";
/// Timeout (in seconds) for getting a response from the server
const RESP_TIMEOUT: u64 = 5;
/// Date format used for URLs on "dilbert.com"
const SRC_DATE_FMT: &str = "%Y-%m-%d";
/// Path to the directory where test scraping files are stored
const SCRAPING_TEST_CASE_PATH: &str = "testdata/scraping";

/// Get the HTTP client.
fn get_http_client() -> Client {
    let timeout = Duration::from_secs(RESP_TIMEOUT);
    Client::builder()
        .disable_redirects()
        .timeout(timeout)
        .finish()
}

/// Test if an HTTP response is a valid HTML page.
///
/// # Arguments
/// * `resp` - The HTTP response
/// * `expected` - The expected Content-Type header
async fn test_content_type<T>(resp: ClientResponse<T>, expected: &str) {
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

#[test_case("2000-01-01"; "sample date")]
#[actix_web::test]
/// Test whether the homepage gives the latest comic.
///
/// # Arguments
/// * `html_file_stem` - The file stem to the HTML page that is to be served for the latest date.
async fn test_latest_comic(html_file_stem: &str) {
    let port = pick_unused_port().expect("Couldn't find an available port");
    let host = format!("{}:{}", HOST, port);

    // Set up the mock server to serve the comic for the mocked latest date.
    let mock_server = MockServer::start().await;
    let html = tokio::fs::read_to_string(format!(
        "{}/{}.html",
        SCRAPING_TEST_CASE_PATH, html_file_stem
    ))
    .await
    .expect("Couldn't get test page for scraping");
    let today = Utc::now().date_naive();
    Mock::given(method(Method::GET.as_str()))
        .and(path(format!("/strip/{}", today.format(SRC_DATE_FMT))))
        .respond_with(ResponseTemplate::new(StatusCode::OK.as_u16()).set_body_string(html))
        .mount(&mock_server)
        .await;

    // Start the server on a single thread.
    let handle = spawn(run(host.clone(), Some(mock_server.uri()), Some(1)));

    let client = get_http_client();
    let resp = client
        .get(format!("http://{}/", host))
        .send()
        .await
        .expect("Failed to send request to server");

    // Close the server.
    handle.abort();

    assert_eq!(resp.status(), StatusCode::OK, "Response status is not OK",);
    test_content_type(resp, "text/html").await;
}

#[test_case(2000, 1, 1; "valid comic")]
#[test_case(2000, 0, 0; "invalid comic")]
#[actix_web::test]
/// Test a comic webpage.
///
/// # Arguments
/// * `year` - The year of the comic
/// * `month` - The month of the comic
/// * `day` - The day of the comic
async fn test_comic(year: i32, month: u32, day: u32) {
    let port = pick_unused_port().expect("Couldn't find an available port");
    let host = format!("{}:{}", HOST, port);

    let date_str = format!("{:04}-{:02}-{:02}", year, month, day);
    let expected_status = if NaiveDate::from_ymd_opt(year, month, day).is_some() {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    };

    // Set up the mock server along with the HTML content.
    let mock_server = MockServer::start().await;

    // Mock the requested comic, only if it exists.
    if let StatusCode::OK = expected_status {
        let html =
            tokio::fs::read_to_string(format!("{}/{}.html", SCRAPING_TEST_CASE_PATH, date_str))
                .await
                .expect("Couldn't get test page for scraping");
        Mock::given(method(Method::GET.as_str()))
            .and(path(format!("/strip/{}", date_str)))
            .respond_with(ResponseTemplate::new(StatusCode::OK.as_u16()).set_body_string(html))
            .mount(&mock_server)
            .await;
    }

    // Mock the latest date.
    let today = Utc::now().date_naive();
    Mock::given(method(Method::GET.as_str()))
        .and(path(format!("/strip/{}", today.format(SRC_DATE_FMT))))
        // Response body shouldn't matter, so keep it empty.
        .respond_with(ResponseTemplate::new(StatusCode::OK.as_u16()))
        .mount(&mock_server)
        .await;

    // Start the server on a single thread.
    let handle = spawn(run(host.clone(), Some(mock_server.uri()), Some(1)));

    let client = get_http_client();
    let resp = client
        .get(format!("http://{}/{}", host, date_str))
        .send()
        .await
        .expect("Failed to send request to server");

    // Close the server.
    handle.abort();

    assert_eq!(resp.status(), expected_status, "Unexpected response status",);
    if let StatusCode::OK = expected_status {
        test_content_type(resp, "text/html").await;
    }
}
