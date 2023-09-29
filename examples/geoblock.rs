use {
    hyper::{Body, Request, Response, StatusCode},
    std::{convert::Infallible, net::IpAddr},
    tower::{Service, ServiceBuilder, ServiceExt},
    wc::geoip::{
        block::{middleware::GeoBlockLayer, BlockingPolicy},
        maxminddb::geoip2,
        LocalResolver,
    },
};

async fn handle(_request: Request<Body>) -> Result<Response<Body>, Infallible> {
    Ok(Response::new(Body::empty()))
}

fn resolve_ip(_addr: IpAddr) -> geoip2::City<'static> {
    geoip2::City {
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let resolver: LocalResolver = LocalResolver::new(Some(|caller| resolve_ip(caller)), None);
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
