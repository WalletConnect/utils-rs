use {
    future::FutureExt,
    metrics::TaskMetrics,
    std::{future::Future, time::Duration},
};

/// Global `hyper` service task executor that uses the `tokio` runtime and adds
/// metrics for the executed tasks.
#[derive(Default, Clone)]
pub struct ServiceTaskExecutor {
    timeout: Option<Duration>,
    metrics_name: &'static str,
}

impl ServiceTaskExecutor {
    pub fn new() -> Self {
        Default::default()
    }

    /// Optional `task_name` metrics attribute.
    pub fn name(self, metrics_name: Option<&'static str>) -> Self {
        Self {
            timeout: self.timeout,
            metrics_name: metrics_name.unwrap_or(""),
        }
    }

    /// Apply a timeout to all service tasks to prevent them from becoming
    /// zombies for various reasons.
    ///
    /// Default is no timeout.
    pub fn timeout(self, timeout: Option<Duration>) -> Self {
        Self {
            timeout,
            metrics_name: self.metrics_name,
        }
    }
}

impl<F> hyper::rt::Executor<F> for ServiceTaskExecutor
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    fn execute(&self, fut: F) {
        static METRICS: TaskMetrics = TaskMetrics::new("hyper_service_task");

        let fut = fut.with_metrics(METRICS.with_name(self.metrics_name));
        let timeout = self.timeout;

        tokio::spawn(async move {
            if let Some(timeout) = timeout {
                let _ = fut.with_timeout(timeout).await;
            } else {
                fut.await;
            }
        });
    }
}
