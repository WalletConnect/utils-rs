pub use dhat::*;
use {std::time::Duration, tokio::sync::Mutex};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("Profiler is already running")]
pub struct AlreadyRunningError;

static PROFILER_LOCK: Mutex<()> = Mutex::const_new(());

/// Records a DHAT profile for the specified duration, and returns a
/// JSON-serialized profile data.
///
/// Returns an error if a profile is already being recorded.
pub async fn record(duration: Duration) -> Result<String, AlreadyRunningError> {
    let _lock = PROFILER_LOCK.try_lock().map_err(|_| AlreadyRunningError)?;
    let profiler = dhat::Profiler::new_heap();

    // Let the profiler run for the specified duration.
    tokio::time::sleep(duration).await;

    Ok(profiler.finish())
}

#[cfg(test)]
mod test {
    use {super::record, std::time::Duration};

    #[tokio::test]
    async fn profiler_lock() {
        let profile1 = tokio::spawn(record(Duration::from_millis(500)));
        tokio::time::sleep(Duration::from_millis(100)).await;

        let profile2 = record(Duration::from_millis(500)).await;
        assert!(profile2.is_err());

        let profile1_output = profile1.await.unwrap().unwrap();
        assert!(!profile1_output.is_empty());
    }
}
