use {
    crate::{geoip, BlockingPolicy, GeoBlockLayer},
    hyper::{Body, Request, Response, StatusCode},
    std::{
        convert::Infallible,
        net::{IpAddr, Ipv4Addr},
    },
    tower::{Service, ServiceBuilder, ServiceExt},
};

async fn handle(_request: Request<Body>) -> Result<Response<Body>, Infallible> {
    Ok(Response::new(Body::empty()))
}

fn resolve_ip(caller: IpAddr) -> geoip::GeoData {
    if IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)) == caller {
        geoip::GeoData {
            continent: Some("Asia".to_string().into()),
            country: Some("Derkaderkastan".to_string().into()),
            region: None,
            city: None,
        }
    } else {
        geoip::GeoData {
            continent: Some("North America".to_string().into()),
            country: Some("United States".to_string().into()),
            region: None,
            city: None,
        }
    }
}

#[tokio::test]
async fn test_blocked_country() {
    let resolver: geoip::local::LocalResolver = geoip::local::LocalResolver::new(resolve_ip);
    let blocked_countries = vec!["Derkaderkastan".into(), "Quran".into(), "Tristan".into()];

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
    let resolver: geoip::local::LocalResolver = geoip::local::LocalResolver::new(resolve_ip);
    let blocked_countries = vec!["Quran".into(), "Tristan".into()];

    let geoblock = GeoBlockLayer::new(resolver, blocked_countries, BlockingPolicy::Block);

    let mut service = ServiceBuilder::new().layer(geoblock).service_fn(handle);

    let request = Request::builder()
        .header("X-Forwarded-For", "127.0.0.1")
        .body(Body::empty())
        .unwrap();

    let response = service.ready().await.unwrap().call(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
