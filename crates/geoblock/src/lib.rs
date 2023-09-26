//! Middleware which adds geo-location IP blocking.
//!
//! Note: this middleware requires you to use
//! [Router::into_make_service_with_connect_info](https://docs.rs/axum/latest/axum/struct.Router.html#method.into_make_service_with_connect_info)
//! to run your app otherwise it will fail at runtime.
//!
//! See [Router::into_make_service_with_connect_info](https://docs.rs/axum/latest/axum/struct.Router.html#method.into_make_service_with_connect_info) for more details.
pub use geoip;
use {
    axum_client_ip::InsecureClientIp,
    geoip::GeoIpResolver,
    http_body::Body,
    hyper::{Request, Response, StatusCode},
    pin_project::pin_project,
    std::{
        future::Future,
        net::IpAddr,
        pin::Pin,
        task::{Context, Poll},
    },
    thiserror::Error,
    tower::Service,
    tower_layer::Layer,
    tracing::{error, info},
};

#[cfg(test)]
mod tests;

/// Values used to configure the middleware behavior when country information
/// could not be retrieved.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BlockingPolicy {
    Block,
    AllowMissingCountry,
    AllowExtractFailure,
    AllowAll,
}

#[derive(Debug, Error)]
enum GeoBlockError {
    #[error("Country is blocked")]
    Blocked,

    #[error("Unable to extract IP address")]
    UnableToExtractIPAddress,

    #[error("Unable to extract geo data from IP address")]
    UnableToExtractGeoData,

    #[error("Country could not be found in database")]
    CountryNotFound,
}

/// Layer that applies the GeoBlock middleware which blocks requests base on IP
/// geo-location.
#[derive(Debug, Clone)]
#[must_use]
pub struct GeoBlockLayer<R>
where
    R: GeoIpResolver,
{
    blocked_countries: Vec<String>,
    ip_resolver: R,
    blocking_policy: BlockingPolicy,
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
            ip_resolver,
            blocked_countries,
            blocking_policy,
        }
    }
}

impl<S, R> Layer<S> for GeoBlockLayer<R>
where
    R: GeoIpResolver,
{
    type Service = GeoBlockService<S, R>;

    fn layer(&self, inner: S) -> Self::Service {
        GeoBlockService::new(
            inner,
            self.ip_resolver.clone(),
            self.blocked_countries.clone(),
            self.blocking_policy,
        )
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
    inner: S,
    blocked_countries: Vec<String>,
    ip_resolver: R,
    blocking_policy: BlockingPolicy,
}

impl<S, R> GeoBlockService<S, R>
where
    R: GeoIpResolver,
{
    pub fn new(
        inner: S,
        ip_resolver: R,
        blocked_countries: Vec<String>,
        blocking_policy: BlockingPolicy,
    ) -> Self {
        Self {
            inner,
            blocking_policy,
            blocked_countries,
            ip_resolver,
        }
    }

    /// Extracts the IP address from the request.
    fn extract_ip<ReqBody>(&self, req: &Request<ReqBody>) -> Result<IpAddr, GeoBlockError> {
        let client_ip = InsecureClientIp::from(req.headers(), req.extensions())
            .map_err(|_| GeoBlockError::UnableToExtractIPAddress)?;
        Ok(client_ip.0)
    }

    /// Checks if the specified IP address is allowed to access the service.
    fn check_ip(&self, caller: IpAddr) -> Result<(), GeoBlockError> {
        let country = self
            .ip_resolver
            .lookup_geo_data(caller)
            .map_err(|_| GeoBlockError::UnableToExtractGeoData)?
            .country
            .ok_or(GeoBlockError::CountryNotFound)?;

        if self
            .blocked_countries
            .iter()
            .any(|blocked_country| *blocked_country == *country)
        {
            Err(GeoBlockError::Blocked)
        } else {
            Ok(())
        }
    }

    fn check_caller<ReqBody>(&self, req: &Request<ReqBody>) -> Result<(), GeoBlockError> {
        self.check_ip(self.extract_ip(req)?)
    }
}

impl<S, R, ReqBody, ResBody> Service<Request<ReqBody>> for GeoBlockService<S, R>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>>,
    R: GeoIpResolver,
    ResBody: Body + Default,
{
    type Error = S::Error;
    type Future = ResponseFuture<S::Future, ResBody>;
    type Response = S::Response;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<ReqBody>) -> Self::Future {
        match self.check_caller(&request) {
            Ok(_) => ResponseFuture::future(self.inner.call(request)),

            Err(GeoBlockError::Blocked) => {
                let mut res = Response::new(ResBody::default());
                *res.status_mut() = StatusCode::UNAUTHORIZED;
                ResponseFuture::error(res)
            }

            Err(GeoBlockError::UnableToExtractIPAddress)
            | Err(GeoBlockError::UnableToExtractGeoData) => {
                if self.blocking_policy == BlockingPolicy::AllowExtractFailure {
                    info!("Unable to extract client IP address");
                    return ResponseFuture::future(self.inner.call(request));
                }

                let mut res = Response::new(ResBody::default());
                *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                ResponseFuture::error(res)
            }

            Err(e) => {
                if self.blocking_policy == BlockingPolicy::AllowMissingCountry {
                    error!("Unable to extract client IP address: {}", e);
                    return ResponseFuture::future(self.inner.call(request));
                }

                let mut res = Response::new(ResBody::default());
                *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                ResponseFuture::error(res)
            }
        }
    }
}

/// Response future for [`GeoBlock`].
#[pin_project]
pub struct ResponseFuture<F, B> {
    #[pin]
    inner: Kind<F, B>,
}

#[pin_project(project = KindProj)]
enum Kind<F, B> {
    Future {
        #[pin]
        future: F,
    },
    Error {
        response: Option<Response<B>>,
    },
}

impl<F, B> ResponseFuture<F, B> {
    fn future(future: F) -> Self {
        Self {
            inner: Kind::Future { future },
        }
    }

    fn error(res: Response<B>) -> Self {
        Self {
            inner: Kind::Error {
                response: Some(res),
            },
        }
    }
}

impl<F, B, E> Future for ResponseFuture<F, B>
where
    F: Future<Output = Result<Response<B>, E>>,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.project().inner.project() {
            KindProj::Future { future } => future.poll(cx),
            KindProj::Error { response } => {
                let response = response.take().unwrap();
                Poll::Ready(Ok(response))
            }
        }
    }
}
