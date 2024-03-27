use {
    chrono::Duration,
    deadpool_redis::{Pool, PoolError},
    moka::future::Cache,
    redis::{RedisError, Script},
    std::{collections::HashMap, sync::Arc},
};

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
    now_millis: i64,
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
        now_millis,
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
    now_millis: i64,
) -> Result<HashMap<String, (i64, u64)>, InternalRateLimitError> {
    // Remaining is number of tokens remaining. -1 for rate limited.
    // Reset is the time at which there will be 1 more token than before. This
    // could, for example, be used to cache a 0 token count.
    Script::new(include_str!("token_bucket.lua"))
        .key(keys)
        .arg(max_tokens)
        .arg(interval.num_milliseconds())
        .arg(refill_rate)
        .arg(now_millis)
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
    const MAX_TOKENS: u32 = 5;
    const REFILL_RATE: u32 = 1;

    use {
        super::*,
        chrono::Utc,
        deadpool_redis::{Config, Runtime},
        redis::AsyncCommands,
        tokio::time::sleep,
        uuid::Uuid,
    };

    async fn redis_clear_keys(conn_uri: &str, keys: &[String]) {
        let client = redis::Client::open(conn_uri).unwrap();
        let mut conn = client.get_async_connection().await.unwrap();
        for key in keys {
            let _: () = conn.del(key).await.unwrap();
        }
    }

    async fn test_rate_limiting(key: String) {
        let cfg = Config::from_url(REDIS_URI);
        let pool = Arc::new(cfg.create_pool(Some(Runtime::Tokio1)).unwrap());
        let refill_interval = chrono::Duration::try_milliseconds(REFILL_INTERVAL_MILLIS).unwrap();
        let rate_limit = |now_millis: i64| {
            let key = key.clone();
            let pool = pool.clone();
            async move {
                token_bucket_many(
                    &pool,
                    vec![key.clone()],
                    MAX_TOKENS,
                    refill_interval,
                    REFILL_RATE,
                    now_millis,
                )
                .await
                .unwrap()
                .get(&key)
                .unwrap()
                .to_owned()
            }
        };
        // Function to call rate limit multiple times and assert results
        // for tokens count and reset timestamp
        let call_rate_limit_loop = |loop_iterations| async move {
            let first_call_millis = Utc::now().timestamp_millis();
            for i in 0..=loop_iterations {
                let curr_iter = loop_iterations as i64 - i as i64 - 1;

                // Using the first call timestamp for the first call or produce the current
                let result = if i == 0 {
                    rate_limit(first_call_millis).await
                } else {
                    rate_limit(Utc::now().timestamp_millis()).await
                };

                // Assert the remaining tokens count
                assert_eq!(result.0, curr_iter);
                // Assert the reset timestamp should be the first call timestamp + refill
                // interval
                assert_eq!(
                    result.1,
                    (first_call_millis + REFILL_INTERVAL_MILLIS) as u64
                );
            }
            // Returning the refill timestamp
            first_call_millis + REFILL_INTERVAL_MILLIS
        };

        // Call rate limit until max tokens limit is reached
        call_rate_limit_loop(MAX_TOKENS).await;

        // Sleep for the full refill and try again
        // Tokens numbers should be the same as the previous iteration because
        // they were fully refilled
        sleep((refill_interval * MAX_TOKENS as i32).to_std().unwrap()).await;
        let last_timestamp = call_rate_limit_loop(MAX_TOKENS).await;

        // Sleep for just one refill and try again
        // The result must contain one token and the reset timestamp should be
        // the last full iteration call timestamp + refill interval
        sleep((refill_interval).to_std().unwrap()).await;
        let result = rate_limit(Utc::now().timestamp_millis()).await;
        assert_eq!(result.0, 0);
        assert_eq!(result.1, (last_timestamp + REFILL_INTERVAL_MILLIS) as u64);
    }

    #[tokio::test]
    async fn test_token_bucket_many() {
        const KEYS_NUMBER_TO_TEST: usize = 3;
        let keys = (0..KEYS_NUMBER_TO_TEST)
            .map(|_| Uuid::new_v4().to_string())
            .collect::<Vec<String>>();

        // Before running the test, ensure the test keys are cleared
        redis_clear_keys(REDIS_URI, &keys).await;

        // Start async test for each key and wait for all to complete
        let tasks = keys.iter().map(|key| test_rate_limiting(key.clone()));
        futures::future::join_all(tasks).await;

        // Clear keys after the test
        redis_clear_keys(REDIS_URI, &keys).await;
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
        let key = Uuid::new_v4().to_string();

        // Before running the test, ensure the test keys are cleared
        redis_clear_keys(REDIS_URI, &[key.clone()]).await;

        let refill_interval = chrono::Duration::try_milliseconds(REFILL_INTERVAL_MILLIS).unwrap();
        let rate_limit = |now_millis| {
            let key = key.clone();
            let pool = pool.clone();
            let cache = cache.clone();
            async move {
                token_bucket(
                    &cache,
                    &pool,
                    key.clone(),
                    MAX_TOKENS,
                    refill_interval,
                    REFILL_RATE,
                    now_millis,
                )
                .await
            }
        };
        let call_rate_limit_loop = |now_millis| async move {
            for i in 0..=MAX_TOKENS {
                let result = rate_limit(now_millis).await;
                if i == MAX_TOKENS {
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
        call_rate_limit_loop(Utc::now().timestamp_millis()).await;

        // Sleep for refill and try again
        sleep((refill_interval * MAX_TOKENS as i32).to_std().unwrap()).await;
        call_rate_limit_loop(Utc::now().timestamp_millis()).await;

        // Clear keys after the test
        redis_clear_keys(REDIS_URI, &[key.clone()]).await;
    }
}
