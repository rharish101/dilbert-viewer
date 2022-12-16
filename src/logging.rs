//! Customization for the request logger middleware
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
use actix_web::{
    dev::{ServiceRequest, ServiceResponse},
    http::header::{REFERER, USER_AGENT},
    HttpMessage, Result as ActixResult,
};
use tracing::{info, info_span, Span};
use tracing_actix_web::{RequestId, RootSpanBuilder};

use crate::constants::TELEMETRY_TARGET;

pub struct RequestSpanBuilder {}

impl RootSpanBuilder for RequestSpanBuilder {
    fn on_request_start(request: &ServiceRequest) -> Span {
        let request_id = request
            .extensions()
            .get::<RequestId>()
            .cloned()
            .expect("Missing request ID");
        let span = info_span!("request", id=%request_id);

        let ip_addr = request
            .connection_info()
            .realip_remote_addr()
            .unwrap_or("-")
            .to_string();

        // The first line of the HTTP request
        let query = request.query_string();
        let req_line = format!(
            "{} {}{} {:?}",
            request.method(),
            request.path(),
            if query.is_empty() {
                String::new()
            } else {
                format!("?{query}")
            },
            request.version(),
        );

        let extract_header = |name| {
            if let Some(value) = request.headers().get(name) {
                value.to_str().unwrap_or("-")
            } else {
                "-"
            }
        };
        let user_agent = extract_header(USER_AGENT);
        let referrer = extract_header(REFERER);

        // Record some request info within the "telemetry" target, so that the log level for
        // telemetry can be independently set.
        info!(
            target: TELEMETRY_TARGET,
            parent: &span,
            ip_addr,
            req_line,
            user_agent,
            referrer
        );

        span
    }

    fn on_request_end<B>(_span: Span, _outcome: &ActixResult<ServiceResponse<B>>) {}
}
