// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

// SPDX-FileCopyrightText: 2020 Luca Palmieri <contact@lpalmieri.com>
//
// SPDX-License-Identifier: MIT

use std::future::{ready, Future, Ready};
use std::pin::Pin;
use std::task::{Context, Poll};

use actix_web::{
    body::{BodySize, MessageBody},
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    web::Bytes,
    Error,
};
use pin_project::{pin_project, pinned_drop};
use tracing::{info_span, Span};
use uuid::Uuid;

#[derive(Default)]
/// Wrapper for encapsulating all log events within a response to a request inside a span
///
/// This span will have a field that contains the unique ID for each request, which is used to
/// distinguish log events for different request-responses.
pub struct TracingWrapper;

impl<S, B> Transform<S, ServiceRequest> for TracingWrapper
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<StreamSpan<B>>;
    type Error = Error;
    type Transform = TracingMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(TracingMiddleware { service }))
    }
}

pub struct TracingMiddleware<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for TracingMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<StreamSpan<B>>;
    type Error = Error;
    type Future = TracingResponse<S::Future>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let root_span = info_span!("request", id=%Uuid::new_v4());
        let fut = root_span.in_scope(|| self.service.call(req));

        TracingResponse {
            fut,
            span: root_span,
        }
    }
}

#[pin_project]
pub struct TracingResponse<F> {
    #[pin]
    fut: F,
    span: Span,
}

#[pin_project(project = PinOptionProj)]
/// A pinned version of `Option`, used for pinning the optional inner response body
enum PinOption<T> {
    None,
    Some(#[pin] T),
}

#[pin_project(PinnedDrop)]
pub struct StreamSpan<B> {
    #[pin]
    body: PinOption<B>,
    span: Span,
}

impl<F, B> Future for TracingResponse<F>
where
    F: Future<Output = Result<ServiceResponse<B>, Error>>,
    B: MessageBody + 'static,
{
    type Output = Result<ServiceResponse<StreamSpan<B>>, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        let fut = this.fut;
        let span = this.span;

        span.in_scope(|| match fut.poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(outcome) => Poll::Ready(outcome.map(|service_response| {
                service_response.map_body(|_, body| StreamSpan {
                    body: PinOption::Some(body),
                    span: span.clone(),
                })
            })),
        })
    }
}

impl<B> MessageBody for StreamSpan<B>
where
    B: MessageBody,
{
    type Error = B::Error;

    fn size(&self) -> BodySize {
        match &self.body {
            PinOption::None => BodySize::None,
            PinOption::Some(body) => body.size(),
        }
    }

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Bytes, Self::Error>>> {
        let this = self.project();

        let body = this.body;
        let span = this.span;
        match body.project() {
            PinOptionProj::None => Poll::Ready(None),
            PinOptionProj::Some(body) => span.in_scope(|| body.poll_next(cx)),
        }
    }
}

#[pinned_drop]
impl<B> PinnedDrop for StreamSpan<B> {
    /// Drop the inner body within the span by assigning `PinOption::None` to it.
    // This is used to wrap `actix_web::middleware::Logger`'s log messages, since they log when
    // they are dropped.
    fn drop(self: Pin<&mut Self>) {
        let this = self.project();

        let span = this.span;
        let mut body = this.body;
        span.in_scope(|| {
            body.set(PinOption::None);
        });
    }
}
