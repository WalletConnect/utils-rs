use {
    chrono::{DateTime, Duration, Utc},
    core::fmt,
    deadpool_redis::{Pool, PoolError},
    moka::future::Cache,
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

/// Rate limit check using a token bucket algorithm for one key and in-memory
/// cache for rate-limited keys. `mem_cache` TTL must be set to the same value
/// as the refill interval.
pub async fn token_bucket(
    mem_cache: &Cache<String, u64>,
    redis_write_pool: &Arc<Pool>,
    key: String,
    max_tokens: u32,
    interval: Duration,
    refill_rate: u32,
) -> Result<(), RateLimitError> {
    // Check if the key is in the memory cache of rate limited keys
    // to omit the redis RTT in case of flood
    if let Some(reset) = mem_cache.get(&key).await {
        return Err(RateLimitError::RateLimitExceeded(RateLimitExceeded {
            reset,
        }));
    }

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
        let reset_interval = reset / 1000;

        // Insert the rate-limited key into the memory cache to avoid the redis RTT in
        // case of flood
        mem_cache.insert(key, reset_interval).await;

        Err(RateLimitError::RateLimitExceeded(RateLimitExceeded {
            reset: reset_interval,
        }))
    } else {
        Ok(())
    }
}

/// Rate limit check using a token bucket algorithm for many keys.
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
    const REFILL_INTERVAL_MILLIS: i64 = 100;

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
    async fn test_token_bucket_many() {
        let cfg = Config::from_url(REDIS_URI);
        let pool = Arc::new(cfg.create_pool(Some(Runtime::Tokio1)).unwrap());
        let key = "token_bucket_many_test_key".to_string();

        // Before running the test, ensure the test keys are cleared
        redis_clear_keys(REDIS_URI, &[key.clone()]).await;

        let max_tokens = 10;
        let refill_interval = chrono::Duration::try_milliseconds(REFILL_INTERVAL_MILLIS).unwrap();
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
        let call_rate_limit_loop = || async {
            for i in 0..=max_tokens {
                let curr_iter = max_tokens as i64 - i as i64 - 1;
                let result = rate_limit().await;
                assert_eq!(result.0, curr_iter);
            }
        };

        // Call rate limit until max tokens limit is reached
        call_rate_limit_loop().await;

        // Sleep for refill and try again
        // Tokens numbers should be the same as the previous iteration because
        // they were refilled
        sleep((refill_interval * max_tokens as i32).to_std().unwrap()).await;
        call_rate_limit_loop().await;

        // Clear keys after the test
        redis_clear_keys(REDIS_URI, &[key.clone()]).await;
    }

    #[tokio::test]
    async fn test_token_bucket() {
        // Create Moka cache with a TTL of the refill interval
        let cache: Cache<String, u64> = Cache::builder()
            .time_to_live(std::time::Duration::from_millis(
                REFILL_INTERVAL_MILLIS as u64,
            ))
            .build();

        let cfg = Config::from_url(REDIS_URI);
        let pool = Arc::new(cfg.create_pool(Some(Runtime::Tokio1)).unwrap());
        let key = "token_bucket_test_key".to_string();

        // Before running the test, ensure the test keys are cleared
        redis_clear_keys(REDIS_URI, &[key.clone()]).await;

        let max_tokens = 10;
        let refill_interval = chrono::Duration::try_milliseconds(REFILL_INTERVAL_MILLIS).unwrap();
        let refill_rate = 1;
        let rate_limit = || async {
            token_bucket(
                &cache,
                &pool,
                key.clone(),
                max_tokens,
                refill_interval,
                refill_rate,
            )
            .await
        };
        let call_rate_limit_loop = || async {
            for i in 0..=max_tokens {
                let result = rate_limit().await;
                if i == max_tokens {
                    assert!(result
                        .err()
                        .unwrap()
                        .to_string()
                        .contains("Rate limit exceeded"));
                } else {
                    assert!(result.is_ok());
                }
            }
        };

        // Call rate limit until max tokens limit is reached
        call_rate_limit_loop().await;

        // Sleep for refill and try again
        sleep((refill_interval * max_tokens as i32).to_std().unwrap()).await;
        call_rate_limit_loop().await;

        // Clear keys after the test
        redis_clear_keys(REDIS_URI, &[key.clone()]).await;
    }
}
