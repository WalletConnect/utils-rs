use {
    geoblock::geoip::GeoData,
    hyper::{Body, Request, Response, StatusCode},
    std::{
        convert::Infallible,
        net::{IpAddr, Ipv4Addr},
    },
    tower::{Service, ServiceBuilder, ServiceExt},
    wc::geoblock::{geoip::local::LocalResolver, GeoBlockLayer},
};

async fn handle(_request: Request<Body>) -> Result<Response<Body>, Infallible> {
    Ok(Response::new(Body::empty()))
}

fn resolve_ip(caller: IpAddr) -> GeoData {
    if IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)) == caller {
        GeoData {
            continent: Some("Asia".to_string().into()),
            country: Some("Derkaderkastan".to_string().into()),
            region: None,
            city: None,
        }
    } else {
        GeoData {
            continent: Some("North America".to_string().into()),
            country: Some("United States".to_string().into()),
            region: None,
            city: None,
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let resolver: LocalResolver = LocalResolver::new(|caller| resolve_ip(caller));
    let blocked_countries = vec![
        "Derkaderkastan".to_string(),
        "Quran".to_string(),
        "Tristan".to_string(),
    ];

    let geoblock = GeoBlockLayer::new(resolver, blocked_countries);

    let mut service = ServiceBuilder::new().layer(geoblock).service_fn(handle);

    let request = Request::builder().body(Body::empty()).unwrap();

    let response = service.ready().await?.call(request).await?;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    Ok(())
}
