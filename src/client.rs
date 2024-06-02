// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! HTTP client for scraping requested Dilbert comics
use std::time::Duration;

use awc::{Client, ClientRequest};

use crate::constants::RESP_TIMEOUT;

/// An HTTP client wrapper for a certain fixed base URL.
///
/// Allowing the base URL to change is useful when mocking it in tests.
pub struct HttpClient {
    client: Client,
}

impl HttpClient {
    /// Initialize the HTTP client session.
    pub fn new() -> Self {
        let timeout = Duration::from_secs(RESP_TIMEOUT);
        let client = Client::builder().timeout(timeout).finish();
        Self { client }
    }

    /// Perform a GET request for the given URL path.
    pub fn get(&self, path: &str) -> ClientRequest {
        self.client.get(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use actix_web::http::{Method, StatusCode};
    use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

    #[actix_web::test]
    /// Test whether the HTTP client can actually connect to a server.
    async fn test_http_client() {
        let mock_server = MockServer::start().await;
        // Respond to all GET requests with status OK.
        Mock::given(method(Method::GET.as_str()))
            .respond_with(ResponseTemplate::new(StatusCode::OK.as_u16()))
            .mount(&mock_server)
            .await;

        // See if the client can actually connect and get a response.
        let http_client = HttpClient::new();
        let resp = http_client
            .get(&mock_server.uri())
            .send()
            .await
            .expect("Failed to connect to mock server");

        // Sanity check to make sure that we get the response we set.
        assert_eq!(resp.status(), StatusCode::OK, "Response is not status OK");
    }
}
