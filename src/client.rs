//! HTTP client for scraping requested Dilbert comics
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

use awc::{Client, ClientRequest};

use crate::constants::RESP_TIMEOUT;

/// An HTTP client wrapper for a certain fixed base URL.
///
/// Allowing the base URL to change is useful when mocking it in tests.
pub struct HttpClient {
    client: Client,
    base_url: String,
}

impl HttpClient {
    /// Initialize the HTTP client session.
    pub fn new(base_url: String) -> Self {
        let timeout = Duration::from_secs(RESP_TIMEOUT);
        let client = Client::builder()
            .disable_redirects()
            .timeout(timeout)
            .finish();
        Self { client, base_url }
    }

    /// Perform a GET request for the given URL path.
    pub fn get(&self, path: &str) -> ClientRequest {
        self.client.get(format!("{}/{}", self.base_url, path))
    }
}
