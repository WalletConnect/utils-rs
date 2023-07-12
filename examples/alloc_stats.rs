use wc::metrics::ServiceMetrics;

#[global_allocator]
static ALLOCATOR: wc::alloc::Jemalloc = wc::alloc::Jemalloc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ServiceMetrics::init_with_name("metrics_example");

    // Collect allocation stats from Jemalloc and update the metrics.
    wc::alloc::stats::update_jemalloc_metrics().unwrap();

    println!("{}", ServiceMetrics::export().unwrap());

    Ok(())
}
