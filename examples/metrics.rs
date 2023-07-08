use {
    std::time::Duration,
    utils::{
        counter,
        futures::{FutureExt, StaticFutureExt},
        gauge,
        histogram,
        metrics::{ServiceMetrics, TaskMetrics},
    },
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ServiceMetrics::init_with_name("metrics_example");

    gauge!("example_gauge", 150u64);
    histogram!("example_histogram", 150.0);
    counter!("example_counter", 150u64);

    static CORE_TASK_METRICS: TaskMetrics = TaskMetrics::new("core_task");

    async {
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    .with_metrics(CORE_TASK_METRICS.with_name("sleeper"))
    .await;

    async {
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    .spawn("spawned_sleeper")
    .await
    .unwrap();

    println!("{}", ServiceMetrics::export().unwrap());

    Ok(())
}
