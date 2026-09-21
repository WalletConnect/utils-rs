//! Middleware which adds geo-location IP blocking.
//!
//! Note: this middleware requires you to use
//! [Router::into_make_service_with_connect_info](https://docs.rs/axum/latest/axum/struct.Router.html#method.into_make_service_with_connect_info)
//! to run your app otherwise it will fail at runtime.
//!
//! See [Router::into_make_service_with_connect_info](https://docs.rs/axum/latest/axum/struct.Router.html#method.into_make_service_with_connect_info) for more details.

use {
    super::{BlockingPolicy, Error, ZoneFilter},
    crate::Resolver,
    axum_client_ip::InsecureClientIp,
    futures::future::{self, Either, Ready},
    http_body::Body,
    hyper::{http::Extensions, HeaderMap, Request, Response, StatusCode},
    std::{
        fmt,
        net::IpAddr,
        sync::Arc,
        task::{Context, Poll},
    },
    tower::Service,
    tower_layer::Layer,
};

#[cfg(test)]
mod tests;

/// How the middleware decides which address a request came from.
///
/// This choice decides what the geo-block is actually enforcing, so it is
/// explicit rather than implied. A blocked visitor only stays blocked if the
/// address cannot be chosen by the visitor.
#[derive(Clone)]
pub struct ClientIpExtractor(Arc<dyn Fn(&HeaderMap, &Extensions) -> Option<IpAddr> + Send + Sync>);

impl ClientIpExtractor {
    /// Resolve the client address with `f`.
    ///
    /// Return `None` when no address can be established. The configured
    /// [`BlockingPolicy`] then decides whether that allows or blocks the
    /// request, through [`Error::UnableToExtractIPAddress`].
    pub fn new<F>(f: F) -> Self
    where
        F: Fn(&HeaderMap, &Extensions) -> Option<IpAddr> + Send + Sync + 'static,
    {
        Self(Arc::new(f))
    }

    /// Read the address from forwarding headers: the leftmost entry of
    /// `X-Forwarded-For`, then `Forwarded`, `X-Real-Ip`, `Fly-Client-IP`,
    /// `True-Client-IP`, `CF-Connecting-IP` and `CloudFront-Viewer-Address`,
    /// and finally the connection peer.
    ///
    /// **Every header in that chain is set by the caller.** A proxy in front of
    /// this service appends to `X-Forwarded-For` rather than replacing it, so
    /// the leftmost entry stays whatever the caller sent, and the other headers
    /// pass through untouched unless something strips them. A visitor can
    /// therefore name any address and choose the geo answer they want.
    ///
    /// Use this only where no trusted component establishes the caller's
    /// address. Where one does — an API gateway or load balancer that stamps a
    /// header of its own — pass that source through [`ClientIpExtractor::new`]
    /// instead.
    pub fn insecure_from_forwarding_headers() -> Self {
        Self::new(|headers, extensions| {
            InsecureClientIp::from(headers, extensions)
                .ok()
                .map(|client_ip| client_ip.0)
        })
    }

    fn extract(&self, headers: &HeaderMap, extensions: &Extensions) -> Option<IpAddr> {
        (self.0)(headers, extensions)
    }
}

impl fmt::Debug for ClientIpExtractor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ClientIpExtractor").finish_non_exhaustive()
    }
}

#[derive(Debug)]
struct Inner<R> {
    filter: ZoneFilter,
    ip_resolver: R,
    client_ip: ClientIpExtractor,
}

impl<R> Inner<R>
where
    R: Resolver,
{
    fn new(
        ip_resolver: R,
        blocked_zones: Vec<String>,
        blocking_policy: BlockingPolicy,
        client_ip: ClientIpExtractor,
    ) -> Self {
        Self {
            filter: ZoneFilter::new(blocked_zones, blocking_policy),
            ip_resolver,
            client_ip,
        }
    }

    fn check<ReqBody>(&self, request: &Request<ReqBody>) -> Result<(), Error> {
        let client_ip = self
            .client_ip
            .extract(request.headers(), request.extensions())
            .ok_or(Error::UnableToExtractIPAddress)?;

        self.filter.check(client_ip, &self.ip_resolver)
    }
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
    /// Block on an address read from forwarding headers.
    ///
    /// The caller controls those headers — see
    /// [`ClientIpExtractor::insecure_from_forwarding_headers`]. Prefer
    /// [`GeoBlockLayer::with_client_ip_extractor`] wherever something in front
    /// of this service establishes the address.
    pub fn new(
        ip_resolver: R,
        blocked_countries: Vec<String>,
        blocking_policy: BlockingPolicy,
    ) -> Self {
        Self::with_client_ip_extractor(
            ip_resolver,
            blocked_countries,
            blocking_policy,
            ClientIpExtractor::insecure_from_forwarding_headers(),
        )
    }

    /// Block on the address `client_ip` reports, instead of on forwarding
    /// headers.
    pub fn with_client_ip_extractor(
        ip_resolver: R,
        blocked_countries: Vec<String>,
        blocking_policy: BlockingPolicy,
        client_ip: ClientIpExtractor,
    ) -> Self {
        Self {
            inner: Arc::new(Inner::new(
                ip_resolver,
                blocked_countries,
                blocking_policy,
                client_ip,
            )),
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
    /// Block on an address read from forwarding headers.
    ///
    /// The caller controls those headers — see
    /// [`ClientIpExtractor::insecure_from_forwarding_headers`]. Prefer
    /// [`GeoBlockService::with_client_ip_extractor`] wherever something in
    /// front of this service establishes the address.
    pub fn new(
        service: S,
        ip_resolver: R,
        blocked_zones: Vec<String>,
        blocking_policy: BlockingPolicy,
    ) -> Self {
        Self::with_client_ip_extractor(
            service,
            ip_resolver,
            blocked_zones,
            blocking_policy,
            ClientIpExtractor::insecure_from_forwarding_headers(),
        )
    }

    /// Block on the address `client_ip` reports, instead of on forwarding
    /// headers.
    pub fn with_client_ip_extractor(
        service: S,
        ip_resolver: R,
        blocked_zones: Vec<String>,
        blocking_policy: BlockingPolicy,
        client_ip: ClientIpExtractor,
    ) -> Self {
        Self {
            service,
            inner: Arc::new(Inner::new(
                ip_resolver,
                blocked_zones,
                blocking_policy,
                client_ip,
            )),
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

        match inner.filter.apply_policy(inner.check(&request)) {
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
