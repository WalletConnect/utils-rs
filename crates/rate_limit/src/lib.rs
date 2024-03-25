use {
    chrono::{DateTime, Duration, Utc},
    core::fmt,
    deadpool_redis::{Pool, PoolError},
    redis::{RedisError, Script},
    std::{collections::HashMap, sync::Arc},
};

pub type Clock = Option<Arc<dyn ClockImpl>>;
pub trait ClockImpl: fmt::Debug + Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Debug, thiserror::Error)]
#[error("Rate limit exceeded. Try again at {reset}")]
pub struct RateLimitExceeded {
    reset: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum InternalRateLimitError {
    #[error("Redis pool error {0}")]
    Pool(PoolError),

    #[error("Redis error: {0}")]
    Redis(RedisError),
}

#[derive(Debug, thiserror::Error)]
pub enum RateLimitError {
    #[error(transparent)]
    RateLimitExceeded(RateLimitExceeded),

    #[error("Internal error: {0}")]
    Internal(InternalRateLimitError),
}

pub async fn token_bucket(
    redis_write_pool: &Arc<Pool>,
    key: String,
    max_tokens: u32,
    interval: Duration,
    refill_rate: u32,
) -> Result<(), RateLimitError> {
    let result = token_bucket_many(
        redis_write_pool,
        vec![key.clone()],
        max_tokens,
        interval,
        refill_rate,
    )
    .await
    .map_err(RateLimitError::Internal)?;

    let (remaining, reset) = result.get(&key).expect("Should contain the key");
    if remaining.is_negative() {
        Err(RateLimitError::RateLimitExceeded(RateLimitExceeded {
            reset: reset / 1000,
        }))
    } else {
        Ok(())
    }
}

pub async fn token_bucket_many(
    redis_write_pool: &Arc<Pool>,
    keys: Vec<String>,
    max_tokens: u32,
    interval: Duration,
    refill_rate: u32,
) -> Result<HashMap<String, (i64, u64)>, InternalRateLimitError> {
    let now = Utc::now();

    // Remaining is number of tokens remaining. -1 for rate limited.
    // Reset is the time at which there will be 1 more token than before. This
    // could, for example, be used to cache a 0 token count.
    Script::new(include_str!("token_bucket.lua"))
        .key(keys)
        .arg(max_tokens)
        .arg(interval.num_milliseconds())
        .arg(refill_rate)
        .arg(now.timestamp_millis())
        .invoke_async::<_, String>(
            &mut redis_write_pool
                .clone()
                .get()
                .await
                .map_err(InternalRateLimitError::Pool)?,
        )
        .await
        .map_err(InternalRateLimitError::Redis)
        .map(|value| serde_json::from_str(&value).expect("Redis script should return valid JSON"))
}

#[cfg(test)]
mod tests {
    const REDIS_URI: &str = "redis://localhost:6379";
    use {
        super::*,
        deadpool_redis::{Config, Runtime},
        redis::AsyncCommands,
        tokio::time::sleep,
    };

    async fn redis_clear_keys(conn_uri: &str, keys: &[String]) {
        let client = redis::Client::open(conn_uri).unwrap();
        let mut conn = client.get_async_connection().await.unwrap();
        for key in keys {
            let _: () = conn.del(key).await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_token_bucket() {
        let cfg = Config::from_url(REDIS_URI);
        let pool = Arc::new(cfg.create_pool(Some(Runtime::Tokio1)).unwrap());
        let key = "test_token_bucket".to_string();

        // Before running the test, ensure the test keys are cleared
        redis_clear_keys(REDIS_URI, &[key.clone()]).await;

        let max_tokens = 10;
        let refill_interval = chrono::Duration::try_milliseconds(100).unwrap();
        let refill_rate = 1;
        let rate_limit = || async {
            token_bucket_many(
                &pool,
                vec![key.clone()],
                max_tokens,
                refill_interval,
                refill_rate,
            )
            .await
            .unwrap()
            .get(&key.clone())
            .unwrap()
            .to_owned()
        };

        // Iterate over the max tokens
        for i in 0..=max_tokens {
            let curr_iter = max_tokens as i64 - i as i64 - 1;
            let result = rate_limit().await;
            assert_eq!(result.0, curr_iter);
        }

        // Sleep for refill and try again
        // Tokens numbers should be the same as the previous iteration
        sleep((refill_interval * max_tokens as i32).to_std().unwrap()).await;

        for i in 0..=max_tokens {
            let curr_iter = max_tokens as i64 - i as i64 - 1;
            let result = rate_limit().await;
            assert_eq!(result.0, curr_iter);
        }

        // Clear keys after the test
        redis_clear_keys(REDIS_URI, &[key.clone()]).await;
    }
}
