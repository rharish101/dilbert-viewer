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
    body::{BodySize, MessageBody},
    dev::{ServiceRequest, ServiceResponse},
    http::header::{REFERER, USER_AGENT},
    HttpMessage, Result as ActixResult,
};
use chrono::NaiveDateTime;
use tracing::{error, info, info_span, Span};
use tracing_actix_web::{RequestId, RootSpanBuilder};

use crate::constants::TELEMETRY_TARGET;
use crate::datetime::curr_datetime;

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

        // Store the starting time, so that response time can be measured.
        request.extensions_mut().insert(curr_datetime());

        span
    }

    fn on_request_end<B: MessageBody>(span: Span, outcome: &ActixResult<ServiceResponse<B>>) {
        match outcome {
            Ok(response) => {
                let size = match response.response().body().size() {
                    BodySize::Sized(size) => format!("{size}B"),
                    BodySize::Stream => "-".into(),
                    BodySize::None => "0B".into(),
                };
                let time_taken = if let Some(start_time) = response
                    .request()
                    .extensions()
                    .get::<NaiveDateTime>()
                    .cloned()
                {
                    format!("{}", curr_datetime() - start_time)
                } else {
                    // This should never happen, since `Self::on_request_start` should always store
                    // the starting time.
                    "-".into()
                };
                info!(target: TELEMETRY_TARGET, parent: &span, status=%response.status(), size, time_taken);
            }
            Err(error) => {
                error!(target: TELEMETRY_TARGET, parent: &span, error=%error);
            }
        };
    }
}
