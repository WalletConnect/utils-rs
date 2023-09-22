// Middleware which adds geo-location IP blocking.

pub use geoip;
use {
    crate::errors::GeoBlockError,
    axum::extract::ConnectInfo,
    geoip::GeoIpResolver,
    http_body::Body,
    hyper::{Request, Response, StatusCode},
    pin_project::pin_project,
    std::{
        future::Future,
        net::{IpAddr, SocketAddr},
        pin::Pin,
        task::{Context, Poll},
    },
    tower::{Layer, Service},
};

pub mod errors;

/// Layer that applies the GeoBlock middleware which blocks requests base on IP
/// geo-location.
#[derive(Debug, Clone)]
#[must_use]
pub struct GeoBlockLayer<T>
where
    T: GeoIpResolver,
{
    blocked_countries: Vec<String>,
    geoip: T,
}

impl<T> GeoBlockLayer<T>
where
    T: GeoIpResolver,
{
    pub fn new(geoip: T, blocked_countries: Vec<String>) -> Self {
        Self {
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
        GeoBlock::new(inner, self.geoip.clone(), self.blocked_countries.clone())
    }
}

#[derive(Clone, Debug)]
pub struct GeoBlock<S, R>
where
    R: GeoIpResolver,
{
    inner: S,
    blocked_countries: Vec<String>,
    geoip: R,
}

impl<S, R> GeoBlock<S, R>
where
    R: GeoIpResolver,
{
    fn new(inner: S, geoip: R, blocked_countries: Vec<String>) -> Self {
        Self {
            inner,
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
        let geo_data = self
            .geoip
            .lookup_geo_data(caller)
            .map_err(|_| GeoBlockError::UnableToExtractGeoData)?;

        // TODO: let configure how to handle missing country
        let country = geo_data
            .country
            .ok_or(GeoBlockError::CountryNotFound)?
            .to_lowercase();

        let is_blocked = self
            .blocked_countries
            .iter()
            .any(|blocked_country| blocked_country == &country);

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
                Err(_e) => {
                    let mut res = Response::new(ResBody::default());
                    *res.status_mut() = StatusCode::UNAUTHORIZED;
                    ResponseFuture::invalid_ip(res)
                }
            },
            Err(_e) => {
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
