use metrics_exporter_prometheus::PrometheusBuilder;

#[global_allocator]
static ALLOCATOR: wc::alloc::Jemalloc = wc::alloc::Jemalloc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let prometheus = PrometheusBuilder::new()
        .install_recorder()
        .expect("install prometheus recorder");

    // Collect allocation stats from Jemalloc and update the metrics.
    wc::alloc::stats::update_jemalloc_metrics().unwrap();

    println!("{}", prometheus.render());

    Ok(())
}
