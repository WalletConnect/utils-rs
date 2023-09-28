//! Middleware which adds geo-location IP blocking.
//!
//! Note: this middleware requires you to use
//! [Router::into_make_service_with_connect_info](https://docs.rs/axum/latest/axum/struct.Router.html#method.into_make_service_with_connect_info)
//! to run your app otherwise it will fail at runtime.
//!
//! See [Router::into_make_service_with_connect_info](https://docs.rs/axum/latest/axum/struct.Router.html#method.into_make_service_with_connect_info) for more details.

use {
    super::{Blacklist, BlockingPolicy, GeoBlockError},
    crate::resolver::GeoIpResolver,
    axum_client_ip::InsecureClientIp,
    futures::future::{self, Either, Ready},
    http_body::Body,
    hyper::{Request, Response},
    std::{
        sync::Arc,
        task::{Context, Poll},
    },
    tower::Service,
    tower_layer::Layer,
};

#[cfg(test)]
mod tests;

#[derive(Debug)]
struct Inner<R> {
    blacklist: Blacklist,
    ip_resolver: R,
    blocking_policy: BlockingPolicy,
}

/// Layer that applies the GeoBlock middleware which blocks requests base on IP
/// geo-location.
#[derive(Debug, Clone)]
#[must_use]
pub struct GeoBlockLayer<R>
where
    R: GeoIpResolver,
{
    inner: Arc<Inner<R>>,
}

impl<R> GeoBlockLayer<R>
where
    R: GeoIpResolver,
{
    pub fn new(
        ip_resolver: R,
        blocked_countries: Vec<String>,
        blocking_policy: BlockingPolicy,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                blacklist: Blacklist::new(blocked_countries),
                ip_resolver,
                blocking_policy,
            }),
        }
    }
}

impl<S, R> Layer<S> for GeoBlockLayer<R>
where
    R: GeoIpResolver,
{
    type Service = GeoBlockService<S, R>;

    fn layer(&self, service: S) -> Self::Service {
        GeoBlockService {
            service,
            inner: self.inner.clone(),
        }
    }
}

/// Layer that applies the GeoBlock middleware which blocks requests base on IP
/// geo-location.
#[derive(Debug, Clone)]
#[must_use]
pub struct GeoBlockService<S, R>
where
    R: GeoIpResolver,
{
    service: S,
    inner: Arc<Inner<R>>,
}

impl<S, R> GeoBlockService<S, R>
where
    R: GeoIpResolver,
{
    pub fn new(
        service: S,
        ip_resolver: R,
        blocked_countries: Vec<String>,
        blocking_policy: BlockingPolicy,
    ) -> Self {
        Self {
            service,
            inner: Arc::new(Inner {
                blacklist: Blacklist::new(blocked_countries),
                ip_resolver,
                blocking_policy,
            }),
        }
    }
}

impl<S, R, ReqBody, ResBody> Service<Request<ReqBody>> for GeoBlockService<S, R>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>>,
    R: GeoIpResolver,
    ResBody: Body + Default,
{
    type Error = S::Error;
    type Future = Either<S::Future, Ready<Result<Response<ResBody>, S::Error>>>;
    type Response = S::Response;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, request: Request<ReqBody>) -> Self::Future {
        let inner = self.inner.as_ref();

        let result = InsecureClientIp::from(request.headers(), request.extensions())
            .map_err(|_| GeoBlockError::UnableToExtractIPAddress)
            .and_then(|client_ip| inner.blacklist.check(client_ip.0, &inner.ip_resolver));

        match inner.blocking_policy.resolve_http_response(result) {
            Ok(_) => Either::Left(self.service.call(request)),

            Err(code) => Either::Right(future::ok(
                Response::builder()
                    .status(code)
                    .body(ResBody::default())
                    .unwrap(),
            )),
        }
    }
}
