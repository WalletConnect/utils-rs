use {
    crate::{
        block::{middleware::GeoBlockLayer, BlockingPolicy},
        resolver,
    },
    hyper::{Body, Request, Response, StatusCode},
    maxminddb::geoip2,
    std::{convert::Infallible, net::IpAddr},
    tower::{Service, ServiceBuilder, ServiceExt},
};

async fn handle(_request: Request<Body>) -> Result<Response<Body>, Infallible> {
    Ok(Response::new(Body::empty()))
}

fn resolve_ip(_addr: IpAddr) -> resolver::City<'static> {
    resolver::City {
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

#[tokio::test]
async fn test_blocked_country() {
    let resolver: resolver::LocalResolver = resolver::LocalResolver::new(Some(resolve_ip), None);
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
async fn test_non_blocked_country() {
    let resolver: resolver::LocalResolver = resolver::LocalResolver::new(Some(resolve_ip), None);
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
