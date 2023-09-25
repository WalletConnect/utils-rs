/// Middleware which adds geo-location IP blocking.
///
/// Note: this middleware requires you to use
/// [Router::into_make_service_with_connect_info](https://docs.rs/axum/latest/axum/struct.Router.html#method.into_make_service_with_connect_info)
/// to run your app otherwise it will fail at runtime.
///
/// See [Router::into_make_service_with_connect_info](https://docs.rs/axum/latest/axum/struct.Router.html#method.into_make_service_with_connect_info) for more details.
pub use geoip;
#[cfg(feature = "tracing")]
use tracing::{error, info};
use {
    axum::{extract::ConnectInfo, http::HeaderMap},
    geoip::GeoIpResolver,
    http_body::Body,
    hyper::{Request, Response, StatusCode},
    pin_project::pin_project,
    std::{
        future::Future,
        net::{IpAddr, SocketAddr},
        pin::Pin,
        sync::Arc,
        task::{Context, Poll},
    },
    thiserror::Error,
    tower::{Layer, Service},
};

#[cfg(test)]
mod tests;

/// Values used to configure the middleware behavior when country information
/// could not be retrieved.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MissingCountry {
    Allow,
    Block,
}

#[derive(Debug, Error)]
pub enum GeoBlockError {
    #[error("Country is blocked: {country}")]
    BlockedCountry { country: Arc<str> },

    #[error("Unable to extract IP address")]
    UnableToExtractIPAddress,

    #[error("Unable to extract geo data from IP address")]
    UnableToExtractGeoData,

    #[error("Country could not be found in database")]
    CountryNotFound,

    #[error("Other Error")]
    Other {
        code: StatusCode,
        msg: Option<String>,
        headers: Option<HeaderMap>,
    },
}

/// Layer that applies the GeoBlock middleware which blocks requests base on IP
/// geo-location.
#[derive(Debug, Clone)]
#[must_use]
pub struct GeoBlockLayer<T>
where
    T: GeoIpResolver,
{
    missing_country: MissingCountry,
    blocked_countries: Vec<Arc<str>>,
    geoip: T,
}

impl<T> GeoBlockLayer<T>
where
    T: GeoIpResolver,
{
    pub fn new(
        geoip: T,
        blocked_countries: Vec<Arc<str>>,
        missing_country: MissingCountry,
    ) -> Self {
        Self {
            missing_country,
            blocked_countries,
            geoip,
        }
    }
}

impl<S, T> Layer<S> for GeoBlockLayer<T>
where
    T: GeoIpResolver,
{
    type Service = GeoBlock<S, T>;

    fn layer(&self, inner: S) -> Self::Service {
        GeoBlock::new(
            inner,
            self.geoip.clone(),
            self.blocked_countries.clone(),
            self.missing_country,
        )
    }
}

#[derive(Clone, Debug)]
pub struct GeoBlock<S, R>
where
    R: GeoIpResolver,
{
    inner: S,
    missing_country: MissingCountry,
    blocked_countries: Vec<Arc<str>>,
    geoip: R,
}

impl<S, R> GeoBlock<S, R>
where
    R: GeoIpResolver,
{
    fn new(
        inner: S,
        geoip: R,
        blocked_countries: Vec<Arc<str>>,
        missing_country: MissingCountry,
    ) -> Self {
        Self {
            inner,
            missing_country,
            blocked_countries,
            geoip,
        }
    }

    fn extract_ip<ReqBody>(&self, req: &Request<ReqBody>) -> Result<IpAddr, GeoBlockError> {
        req.extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(addr)| addr.ip())
            .ok_or(GeoBlockError::UnableToExtractIPAddress)
    }

    fn check_caller(&self, caller: IpAddr) -> Result<(), GeoBlockError> {
        let country = match self
            .geoip
            .lookup_geo_data(caller)
            .map_err(|_| GeoBlockError::UnableToExtractGeoData)
        {
            Ok(geo_data) => match geo_data.country {
                None if self.missing_country == MissingCountry::Allow => {
                    #[cfg(feature = "tracing")]
                    {
                        info!("Country not found, but allowed");
                    }
                    return Ok(());
                }
                None => {
                    #[cfg(feature = "tracing")]
                    {
                        info!("Country not found");
                    }
                    return Err(GeoBlockError::CountryNotFound);
                }
                Some(country) => country,
            },
            Err(_e) => {
                return if self.missing_country == MissingCountry::Allow {
                    Ok(())
                } else {
                    #[cfg(feature = "tracing")]
                    {
                        error!("Unable to extract geo data from IP address: {}", _e);
                    }
                    Err(GeoBlockError::UnableToExtractGeoData)
                }
            }
        };

        let is_blocked = self
            .blocked_countries
            .iter()
            .any(|blocked_country| *blocked_country == country);

        if is_blocked {
            Err(GeoBlockError::BlockedCountry { country })
        } else {
            Ok(())
        }
    }
}

impl<R, S, ReqBody, ResBody> Service<Request<ReqBody>> for GeoBlock<S, R>
where
    R: GeoIpResolver,
    S: Service<Request<ReqBody>, Response = Response<ResBody>>,
    ResBody: Body + Default,
{
    type Error = S::Error;
    type Future = ResponseFuture<S::Future, ResBody>;
    type Response = Response<ResBody>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        match self.extract_ip(&req) {
            Ok(ip_addr) => match self.check_caller(ip_addr) {
                Ok(_) => ResponseFuture::future(self.inner.call(req)),
                Err(GeoBlockError::BlockedCountry { country: _country }) => {
                    let mut res = Response::new(ResBody::default());
                    *res.status_mut() = StatusCode::UNAUTHORIZED;
                    ResponseFuture::invalid_ip(res)
                }
                Err(_e) => {
                    let mut res = Response::new(ResBody::default());
                    *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                    ResponseFuture::invalid_ip(res)
                }
            },
            Err(_e) => {
                if self.missing_country == MissingCountry::Allow {
                    #[cfg(feature = "tracing")]
                    {
                        error!("Unable to extract client IP address: {}", _e);
                    }
                    return ResponseFuture::future(self.inner.call(req));
                }

                let mut res = Response::new(ResBody::default());
                *res.status_mut() = StatusCode::UNAUTHORIZED;
                ResponseFuture::invalid_ip(res)
            }
        }
    }
}

#[pin_project]
/// Response future for [`GeoBlock`].
pub struct ResponseFuture<F, B> {
    #[pin]
    inner: Kind<F, B>,
}

impl<F, B> ResponseFuture<F, B> {
    fn future(future: F) -> Self {
        Self {
            inner: Kind::Future { future },
        }
    }

    fn invalid_ip(res: Response<B>) -> Self {
        Self {
            inner: Kind::Error {
                response: Some(res),
            },
        }
    }
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
