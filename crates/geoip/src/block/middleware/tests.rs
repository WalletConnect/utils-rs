use {
    crate::{
        block::{
            middleware::{ClientIpExtractor, GeoBlockLayer},
            BlockingPolicy,
        },
        LocalResolver,
    },
    axum::body::Body,
    hyper::{Request, Response, StatusCode},
    maxminddb::{geoip2, geoip2::City},
    std::{
        convert::Infallible,
        net::{IpAddr, Ipv4Addr},
        sync::Arc,
    },
    tower::{Service, ServiceBuilder, ServiceExt},
};

/// Resolves to a blocked country.
const BLOCKED_IP: IpAddr = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
/// Resolves to an unblocked country, and is what a caller claims to be.
const CLAIMED_IP: &str = "10.0.0.2";

async fn handle(_request: Request<Body>) -> Result<Response<Body>, Infallible> {
    Ok(Response::new(Body::empty()))
}

fn resolve_ip_no_subs(_addr: IpAddr) -> City<'static> {
    City {
        country: geoip2::city::Country {
            iso_code: Some("CU"),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn resolve_ip(_addr: IpAddr) -> City<'static> {
    City {
        country: geoip2::city::Country {
            iso_code: Some("CU"),
            ..Default::default()
        },
        subdivisions: vec![
            geoip2::city::Subdivision {
                iso_code: Some("12"),
                ..Default::default()
            },
            geoip2::city::Subdivision {
                iso_code: Some("34"),
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

/// Test that a blocking list with no subdivisions blocks the country if
/// a match is found.
#[tokio::test]
async fn test_country_blocked() {
    let resolver = LocalResolver::new(Some(resolve_ip), None);
    let blocked_countries = vec!["CU".into(), "IR".into(), "KP".into()];

    let geoblock = GeoBlockLayer::new(resolver, blocked_countries, BlockingPolicy::Block);

    let mut service = ServiceBuilder::new().layer(geoblock).service_fn(handle);

    let request = Request::builder()
        .header("X-Forwarded-For", "127.0.0.1")
        .body(Body::empty())
        .unwrap();

    let response = service.ready().await.unwrap().call(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Test that a blocking list with no subdivisions doesn't block if the
/// country doesn't match.
#[tokio::test]
async fn test_country_non_blocked() {
    let resolver = LocalResolver::new(Some(resolve_ip), None);
    let blocked_countries = vec!["IR".into(), "KP".into()];

    let geoblock = GeoBlockLayer::new(resolver, blocked_countries, BlockingPolicy::Block);

    let mut service = ServiceBuilder::new().layer(geoblock).service_fn(handle);

    let request = Request::builder()
        .header("X-Forwarded-For", "127.0.0.1")
        .body(Body::empty())
        .unwrap();

    let response = service.ready().await.unwrap().call(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

/// Test that a blocking list with subdivisions doesn't block if the
/// subdivisions don't match, even if the country matches.
#[tokio::test]
async fn test_sub_unblocked_wrong_sub() {
    let resolver = LocalResolver::new(Some(resolve_ip), None);
    let blocked_countries = vec!["CU:56".into(), "IR".into(), "KP".into()];

    let geoblock = GeoBlockLayer::new(&resolver, blocked_countries, BlockingPolicy::Block);

    let mut service = ServiceBuilder::new().layer(geoblock).service_fn(handle);

    let request = Request::builder()
        .header("X-Forwarded-For", "127.0.0.1")
        .body(Body::empty())
        .unwrap();

    let response = service.ready().await.unwrap().call(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

/// Test that a blocking list with subdivisions doesn't block if the country
/// doesn't match, even if subdivisions match.
#[tokio::test]
async fn test_sub_unblocked_wrong_country() {
    let resolver = LocalResolver::new(Some(resolve_ip), None);
    let blocked_countries = vec!["IR:12".into(), "KP".into()];

    let geoblock = GeoBlockLayer::new(&resolver, blocked_countries, BlockingPolicy::Block);

    let mut service = ServiceBuilder::new().layer(geoblock).service_fn(handle);

    let request = Request::builder()
        .header("X-Forwarded-For", "127.0.0.1")
        .body(Body::empty())
        .unwrap();

    let response = service.ready().await.unwrap().call(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

/// Test that a blocking list with subdivisions blocks containing only one
/// subdivision blocks if the country and subdivision match.
#[tokio::test]
async fn test_sub_blocked_country_sub() {
    let resolver = LocalResolver::new(Some(resolve_ip), None);
    let blocked_countries = vec!["CU:12".into(), "IR".into(), "KP".into()];

    let geoblock = GeoBlockLayer::new(&resolver, blocked_countries, BlockingPolicy::Block);

    let mut service = ServiceBuilder::new().layer(geoblock).service_fn(handle);

    let request = Request::builder()
        .header("X-Forwarded-For", "127.0.0.1")
        .body(Body::empty())
        .unwrap();

    let response = service.ready().await.unwrap().call(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Test that a blocking list with subdivisions blocks containing several
/// subdivisions blocks if the country and subdivision match.
#[tokio::test]
async fn test_subs_blocked_country_sub() {
    let resolver = LocalResolver::new(Some(resolve_ip), None);
    let blocked_countries = vec!["CU:12".into(), "CU:34".into(), "IR".into(), "KP".into()];

    let geoblock = GeoBlockLayer::new(&resolver, blocked_countries, BlockingPolicy::Block);

    let mut service = ServiceBuilder::new().layer(geoblock).service_fn(handle);

    let request = Request::builder()
        .header("X-Forwarded-For", "127.0.0.1")
        .body(Body::empty())
        .unwrap();

    let response = service.ready().await.unwrap().call(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Test that a blocking list with subdivisions blocks containing several
/// subdivisions in short form blocks if the country and subdivision match.
#[tokio::test]
async fn test_short_subs_blocked_country_sub() {
    let resolver = LocalResolver::new(Some(resolve_ip), None);
    let blocked_countries = vec!["CU:12:34".into(), "IR".into(), "KP".into()];

    let geoblock = GeoBlockLayer::new(&resolver, blocked_countries, BlockingPolicy::Block);

    let mut service = ServiceBuilder::new().layer(geoblock).service_fn(handle);

    let request = Request::builder()
        .header("X-Forwarded-For", "127.0.0.1")
        .body(Body::empty())
        .unwrap();

    let response = service.ready().await.unwrap().call(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Test that the blocker doesn't crash if the GeoIP resolution doesn't contain
/// any subdivisions.
#[tokio::test]
async fn test_unresolved_subdivisions() {
    let resolver = LocalResolver::new(Some(resolve_ip_no_subs), None);
    let blocked_countries = vec!["CU".into(), "IR".into(), "KP".into()];

    let geoblock = GeoBlockLayer::new(resolver, blocked_countries, BlockingPolicy::Block);

    let mut service = ServiceBuilder::new().layer(geoblock).service_fn(handle);

    let request = Request::builder()
        .header("X-Forwarded-For", "127.0.0.1")
        .body(Body::empty())
        .unwrap();

    let response = service.ready().await.unwrap().call(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_arc() {
    let resolver = Arc::from(LocalResolver::new(Some(resolve_ip), None));
    let blocked_countries = vec!["CU".into(), "IR".into(), "KP".into()];

    let geoblock = GeoBlockLayer::new(&resolver, blocked_countries, BlockingPolicy::Block);

    let mut service = ServiceBuilder::new().layer(geoblock).service_fn(handle);

    let request = Request::builder()
        .header("X-Forwarded-For", "127.0.0.1")
        .body(Body::empty())
        .unwrap();

    let response = service.ready().await.unwrap().call(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Resolves one address to a blocked country and everything else to an
/// unblocked one, so a test can tell which address the middleware actually
/// used rather than only whether it blocked.
fn resolve_by_ip(addr: IpAddr) -> City<'static> {
    let iso_code = if addr == BLOCKED_IP { "CU" } else { "US" };

    City {
        country: geoip2::city::Country {
            iso_code: Some(iso_code),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// A configured extractor decides the outcome, so forwarding headers naming
/// some other country cannot buy a caller its way out of a block.
#[tokio::test]
async fn test_configured_extractor_beats_spoofed_forwarding_headers() {
    let resolver = LocalResolver::new(Some(resolve_by_ip), None);

    let geoblock = GeoBlockLayer::with_client_ip_extractor(
        resolver,
        vec!["CU".into()],
        BlockingPolicy::Block,
        ClientIpExtractor::new(|_headers, _extensions| Some(BLOCKED_IP)),
    );

    let mut service = ServiceBuilder::new().layer(geoblock).service_fn(handle);

    // Every header `InsecureClientIp` consults, all naming an unblocked country.
    let request = Request::builder()
        .header("X-Forwarded-For", CLAIMED_IP)
        .header("Forwarded", format!("for={CLAIMED_IP}"))
        .header("X-Real-Ip", CLAIMED_IP)
        .header("Fly-Client-IP", CLAIMED_IP)
        .header("True-Client-IP", CLAIMED_IP)
        .header("CF-Connecting-IP", CLAIMED_IP)
        .body(Body::empty())
        .unwrap();

    let response = service.ready().await.unwrap().call(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// The header-reading default does the opposite, which is why it is named for
/// what it trusts. Pinned so a dependency bump cannot quietly change which
/// headers decide a block.
#[tokio::test]
async fn test_default_extractor_still_trusts_forwarding_headers() {
    let resolver = LocalResolver::new(Some(resolve_by_ip), None);

    let geoblock = GeoBlockLayer::new(resolver, vec!["CU".into()], BlockingPolicy::Block);

    let mut service = ServiceBuilder::new().layer(geoblock).service_fn(handle);

    let request = Request::builder()
        .header("X-Forwarded-For", CLAIMED_IP)
        .body(Body::empty())
        .unwrap();

    let response = service.ready().await.unwrap().call(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

/// An extractor that establishes no address is an extraction failure, so the
/// blocking policy decides rather than the request sailing through.
#[tokio::test]
async fn test_extractor_without_an_address_defers_to_the_policy() {
    let no_address = || ClientIpExtractor::new(|_headers, _extensions| None);
    let request = || Request::builder().body(Body::empty()).unwrap();

    let blocking = GeoBlockLayer::with_client_ip_extractor(
        LocalResolver::new(Some(resolve_by_ip), None),
        vec!["CU".into()],
        BlockingPolicy::Block,
        no_address(),
    );
    let mut service = ServiceBuilder::new().layer(blocking).service_fn(handle);
    let response = service
        .ready()
        .await
        .unwrap()
        .call(request())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let allowing = GeoBlockLayer::with_client_ip_extractor(
        LocalResolver::new(Some(resolve_by_ip), None),
        vec!["CU".into()],
        BlockingPolicy::AllowExtractFailure,
        no_address(),
    );
    let mut service = ServiceBuilder::new().layer(allowing).service_fn(handle);
    let response = service
        .ready()
        .await
        .unwrap()
        .call(request())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
