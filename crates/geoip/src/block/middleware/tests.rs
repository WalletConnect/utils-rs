use {
    crate::{
        block::{middleware::GeoBlockLayer, BlockingPolicy},
        LocalResolver,
    },
    axum::body::Body,
    http::{Request, Response, StatusCode},
    maxminddb::{geoip2, geoip2::City},
    std::{convert::Infallible, net::IpAddr, sync::Arc},
    tower::{Service, ServiceBuilder, ServiceExt},
};

async fn handle(_request: Request<Body>) -> Result<Response<Body>, Infallible> {
    Ok(Response::new(Body::empty()))
}

fn resolve_ip_no_subs(_addr: IpAddr) -> City<'static> {
    City {
        city: None,
        continent: None,
        country: Some(geoip2::city::Country {
            geoname_id: None,
            is_in_european_union: None,
            iso_code: Some("CU"),
            names: None,
        }),
        location: None,
        postal: None,
        registered_country: None,
        represented_country: None,
        subdivisions: None,
        traits: None,
    }
}

fn resolve_ip(_addr: IpAddr) -> City<'static> {
    City {
        city: None,
        continent: None,
        country: Some(geoip2::city::Country {
            geoname_id: None,
            is_in_european_union: None,
            iso_code: Some("CU"),
            names: None,
        }),
        location: None,
        postal: None,
        registered_country: None,
        represented_country: None,
        subdivisions: Some(vec![
            geoip2::city::Subdivision {
                geoname_id: None,
                iso_code: Some("12"),
                names: None,
            },
            geoip2::city::Subdivision {
                geoname_id: None,
                iso_code: Some("34"),
                names: None,
            },
        ]),
        traits: None,
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
