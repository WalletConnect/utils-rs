use {
    geoblock::{geoip::GeoData, BlockingPolicy, GeoBlockLayer},
    hyper::{Body, Request, Response, StatusCode},
    std::{
        convert::Infallible,
        net::{IpAddr, Ipv4Addr},
    },
    tower::{Service, ServiceBuilder, ServiceExt},
    wc::geoblock::geoip::local::LocalResolver,
};

async fn handle(_request: Request<Body>) -> Result<Response<Body>, Infallible> {
    Ok(Response::new(Body::empty()))
}

fn resolve_ip(caller: IpAddr) -> GeoData {
    if IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)) == caller {
        GeoData {
            continent: Some("NA".to_string().into()),
            country: Some("CU".to_string().into()),
            region: None,
            city: None,
        }
    } else {
        GeoData {
            continent: Some("NA".to_string().into()),
            country: Some("US".to_string().into()),
            region: None,
            city: None,
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let resolver: LocalResolver = LocalResolver::new(|caller| resolve_ip(caller));
    let blocked_countries = vec!["CU".into(), "IR".into(), "KP".into()];

    let geoblock = GeoBlockLayer::new(resolver, blocked_countries, BlockingPolicy::Block);

    let mut service = ServiceBuilder::new().layer(geoblock).service_fn(handle);

    let request = Request::builder()
        .header("X-Forwarded-For", "127.0.0.1")
        .body(Body::empty())
        .unwrap();

    let response = service.ready().await.unwrap().call(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    Ok(())
}
