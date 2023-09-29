//! Middleware which adds geo-location IP blocking.
//!
//! Note: this middleware requires you to use
//! [Router::into_make_service_with_connect_info](https://docs.rs/axum/latest/axum/struct.Router.html#method.into_make_service_with_connect_info)
//! to run your app otherwise it will fail at runtime.
//!
//! See [Router::into_make_service_with_connect_info](https://docs.rs/axum/latest/axum/struct.Router.html#method.into_make_service_with_connect_info) for more details.

use {
    super::{BlockingPolicy, CountryFilter, Error},
    crate::Resolver,
    axum_client_ip::InsecureClientIp,
    futures::future::{self, Either, Ready},
    http_body::Body,
    hyper::{Request, Response, StatusCode},
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
    filter: CountryFilter,
    ip_resolver: R,
}

/// Layer that applies the GeoBlock middleware which blocks requests base on IP
/// geo-location.
#[derive(Debug, Clone)]
#[must_use]
pub struct GeoBlockLayer<R>
where
    R: Resolver,
{
    inner: Arc<Inner<R>>,
}

impl<R> GeoBlockLayer<R>
where
    R: Resolver,
{
    pub fn new(
        ip_resolver: R,
        blocked_countries: Vec<String>,
        blocking_policy: BlockingPolicy,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                filter: CountryFilter::new(blocked_countries, blocking_policy),
                ip_resolver,
            }),
        }
    }
}

impl<S, R> Layer<S> for GeoBlockLayer<R>
where
    R: Resolver,
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
    R: Resolver,
{
    service: S,
    inner: Arc<Inner<R>>,
}

impl<S, R> GeoBlockService<S, R>
where
    R: Resolver,
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
                filter: CountryFilter::new(blocked_countries, blocking_policy),
                ip_resolver,
            }),
        }
    }
}

impl<S, R, ReqBody, ResBody> Service<Request<ReqBody>> for GeoBlockService<S, R>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>>,
    R: Resolver,
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
            .map_err(|_| Error::UnableToExtractIPAddress)
            .and_then(|client_ip| inner.filter.check(client_ip.0, &inner.ip_resolver));

        match inner.filter.apply_policy(result) {
            Ok(_) => Either::Left(self.service.call(request)),

            Err(err) => {
                let code = match err {
                    Error::Blocked => StatusCode::UNAUTHORIZED,
                    Error::UnableToExtractIPAddress
                    | Error::UnableToExtractGeoData
                    | Error::CountryNotFound => {
                        tracing::warn!(?err, "failed to check geoblocking");

                        StatusCode::INTERNAL_SERVER_ERROR
                    }
                };

                let mut response = Response::new(ResBody::default());
                *response.status_mut() = code;

                Either::Right(future::ok(response))
            }
        }
    }
}
