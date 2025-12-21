mod algorithm;
mod command_parser;
mod traffic_policy;

use redis_module::{redis_module, Context, RedisResult, RedisString};

use crate::{command_parser::parse_command_args, traffic_policy::create_executor};

// Redis command configuration
const REDIS_COMMAND: &str = "SHIELD.absorb";
const REDIS_MODULE_NAME: &str = "SHIELD";
const REDIS_MODULE_VERSION: i32 = 1;

#[cfg(not(test))]
macro_rules! get_allocator {
    () => {
        redis_module::alloc::RedisAlloc
    };
}

#[cfg(test)]
macro_rules! get_allocator {
    () => {
        std::alloc::System
    };
}

/// Entry point to `SHIELD.absorb` redis command.
///
/// This command implements a token bucket rate limiting algorithm.
///
/// # Command Format
/// ```
/// SHIELD.absorb <key> <capacity> <period> [tokens]
/// ```
///
/// # Arguments
/// * `key` - Unique identifier for the bucket
/// * `capacity` - Maximum number of tokens the bucket can hold (must be positive)
/// * `period` - Time period in seconds for bucket refill (must be positive)
/// * `tokens` - Number of tokens to consume (optional, defaults to 1, must be positive)
/// * `algorithm` - Rate limiting algorithm to use (optional, defaults to token_bucket, supported: token_bucket, leaky_bucket, fixed_window, sliding_window)
///
/// # Returns
/// * `tokens_remaining` - Number of tokens left in the bucket after consumption
/// * `-1` - If insufficient tokens are available (request denied)
///
/// # Errors
/// * `WrongArity` - If incorrect number of arguments provided
/// * `String` - If arguments are invalid (not positive integers)
/// * Redis errors from underlying operations
///
/// # Examples
/// ```
/// SHIELD.absorb user123 30 60     # Remove 1 token from bucket with capacity 30, period 60s
/// SHIELD.absorb user123 30 60 5   # Remove 5 tokens from the same bucket
/// ```
#[inline]
fn redis_command(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    let command = parse_command_args(&args)?;
    let mut executor = create_executor(command.cfg, ctx, command.key.to_owned())?;
    let result = executor.execute(command.tokens)?;
    Ok(result.into())
}

redis_module! {
    name: REDIS_MODULE_NAME,
    version: REDIS_MODULE_VERSION,
    allocator: (get_allocator!(), get_allocator!()),
    data_types: [],
    commands: [
        [REDIS_COMMAND, redis_command, "", 0, 0, 0],
    ],
}

//////////////////////////////////////////////////////////////////////
// Tests
//////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command_parser::{ARG_ALGORITHM_FLAG, DEFAULT_TOKENS};
    extern crate redis;
    use redis::Commands;
    use std::env;
    use std::{thread, time};

    /// Establishes a connection to Redis using the REDIS_URL environment variable.
    ///
    /// # Panics
    /// Panics if REDIS_URL is not set or connection fails.
    fn establish_connection() -> redis::Connection {
        let redis_url = env::var("REDIS_URL").unwrap();
        let client = redis::Client::open(redis_url).unwrap();
        client.get_connection().unwrap()
    }

    /// Helper function to execute SHIELD.absorb command with the given arguments.
    fn shield_absorb(
        con: &mut redis::Connection,
        key: &str,
        capacity: i64,
        period: i64,
        tokens: Option<i64>,
    ) -> redis::RedisResult<i64> {
        shield_absorb_with_options(con, key, capacity, period, tokens, None)
    }

    /// Helper function to execute SHIELD.absorb with an explicit algorithm selection.
    fn shield_absorb_with_algorithm(
        con: &mut redis::Connection,
        key: &str,
        capacity: i64,
        period: i64,
        tokens: Option<i64>,
        algorithm: &str,
    ) -> redis::RedisResult<i64> {
        shield_absorb_with_options(con, key, capacity, period, tokens, Some(algorithm))
    }

    fn shield_absorb_with_options(
        con: &mut redis::Connection,
        key: &str,
        capacity: i64,
        period: i64,
        tokens: Option<i64>,
        algorithm: Option<&str>,
    ) -> redis::RedisResult<i64> {
        let mut cmd = redis::cmd(REDIS_COMMAND);
        cmd.arg(key).arg(capacity).arg(period);
        if let Some(t) = tokens {
            cmd.arg(t);
        }
        if let Some(algo) = algorithm {
            cmd.arg(ARG_ALGORITHM_FLAG).arg(algo);
        }
        cmd.query(con)
    }

    /// Helper function to clean up test keys.
    fn cleanup_key(con: &mut redis::Connection, key: &str) {
        let _: () = con.del(key).unwrap();
    }

    fn build_redis_key(key: &str) -> String {
        build_redis_key_for_suffix(
            key,
            traffic_policy::PolicyConfig::TokenBucket {
                capacity: 0,
                period: 0,
            }
            .suffix(),
        )
    }

    fn build_redis_key_for_suffix(key: &str, suffix: &str) -> String {
        let key_buf = traffic_policy::build_key(key, suffix);
        key_buf.as_str().to_owned()
    }

    // Test constants for better readability
    const TEST_CAPACITY: i64 = 30;
    const TEST_PERIOD: i64 = 60;

    #[test]
    #[should_panic(
        expected = "ResponseError: wrong number of arguments for 'SHIELD.absorb' command"
    )]
    fn test_wrong_arity() {
        let mut con = establish_connection();
        let _: () = redis::cmd(REDIS_COMMAND).query(&mut con).unwrap();
    }

    #[test]
    #[should_panic(expected = "ResponseError: capacity must be positive")]
    fn test_capacity_is_string() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_capacity_string";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        let _: () = redis::cmd(REDIS_COMMAND)
            .arg(bucket_key)
            .arg("abc")
            .arg(TEST_PERIOD)
            .query(&mut con)
            .unwrap();
    }

    #[test]
    #[should_panic(expected = "ResponseError: capacity must be positive")]
    fn test_capacity_is_float() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_capacity_float";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        let _: () = redis::cmd(REDIS_COMMAND)
            .arg(bucket_key)
            .arg(1.2)
            .arg(TEST_PERIOD)
            .query(&mut con)
            .unwrap();
    }

    #[test]
    #[should_panic(expected = "ResponseError: capacity must be positive")]
    fn test_capacity_is_zero() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_capacity_zero";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        let _ = shield_absorb(&mut con, bucket_key, 0, TEST_PERIOD, None).unwrap();
    }

    #[test]
    #[should_panic(expected = "ResponseError: capacity must be positive")]
    fn test_capacity_is_negative() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_capacity_negative";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        let _ = shield_absorb(&mut con, bucket_key, -2, TEST_PERIOD, None).unwrap();
    }

    #[test]
    #[should_panic(expected = "ResponseError: period/window must be positive")]
    fn test_period_is_string() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_period_string";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        let _: () = redis::cmd(REDIS_COMMAND)
            .arg(bucket_key)
            .arg(TEST_CAPACITY)
            .arg("abc")
            .query(&mut con)
            .unwrap();
    }

    #[test]
    #[should_panic(expected = "ResponseError: period/window must be positive")]
    fn test_period_is_float() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_period_float";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        let _: () = redis::cmd(REDIS_COMMAND)
            .arg(bucket_key)
            .arg(TEST_CAPACITY)
            .arg(3.14)
            .query(&mut con)
            .unwrap();
    }

    #[test]
    #[should_panic(expected = "ResponseError: period/window must be positive")]
    fn test_period_is_zero() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_period_zero";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        let _ = shield_absorb(&mut con, bucket_key, TEST_CAPACITY, 0, None).unwrap();
    }

    #[test]
    #[should_panic(expected = "ResponseError: period/window must be positive")]
    fn test_period_is_negative() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_period_negative";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        let _ = shield_absorb(&mut con, bucket_key, TEST_CAPACITY, -4, None).unwrap();
    }

    #[test]
    #[should_panic(expected = "ResponseError: tokens must be positive")]
    fn test_tokens_is_string() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_tokens_string";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        let _: () = redis::cmd(REDIS_COMMAND)
            .arg(bucket_key)
            .arg(TEST_CAPACITY)
            .arg(TEST_PERIOD)
            .arg("abc")
            .query(&mut con)
            .unwrap();
    }

    #[test]
    #[should_panic(expected = "ResponseError: tokens must be positive")]
    fn test_tokens_is_float() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_tokens_float";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        let _: () = redis::cmd(REDIS_COMMAND)
            .arg(bucket_key)
            .arg(TEST_CAPACITY)
            .arg(TEST_PERIOD)
            .arg(2.5)
            .query(&mut con)
            .unwrap();
    }

    #[test]
    #[should_panic(expected = "ResponseError: tokens must be positive")]
    fn test_tokens_is_zero() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_tokens_zero";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        let _ = shield_absorb(&mut con, bucket_key, TEST_CAPACITY, TEST_PERIOD, Some(0)).unwrap();
    }

    #[test]
    #[should_panic(expected = "ResponseError: tokens must be positive")]
    fn test_tokens_is_negative() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_tokens_negative";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        let _ = shield_absorb(&mut con, bucket_key, TEST_CAPACITY, TEST_PERIOD, Some(-9)).unwrap();
    }

    #[test]
    fn test_default_algorithm_is_token_bucket() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_default_token_bucket";
        let token_bucket_key = build_redis_key(bucket_key);
        let leaky_bucket_key = build_redis_key_for_suffix(
            bucket_key,
            traffic_policy::PolicyConfig::LeakyBucket {
                capacity: 0,
                period: 0,
            }
            .suffix(),
        );

        cleanup_key(&mut con, token_bucket_key.as_str());
        cleanup_key(&mut con, leaky_bucket_key.as_str());

        let remaining_tokens =
            shield_absorb(&mut con, bucket_key, TEST_CAPACITY, TEST_PERIOD, None).unwrap();
        assert_eq!(
            remaining_tokens,
            TEST_CAPACITY - DEFAULT_TOKENS,
            "Default invocation should consume using the token bucket algorithm"
        );

        let token_bucket_exists: bool = con.exists(token_bucket_key.as_str()).unwrap();
        let leaky_bucket_exists: bool = con.exists(leaky_bucket_key.as_str()).unwrap();
        assert!(
            token_bucket_exists,
            "Token bucket storage should be created when no algorithm override is provided"
        );
        assert!(
            !leaky_bucket_exists,
            "Other algorithm storage should remain untouched when using defaults"
        );

        cleanup_key(&mut con, token_bucket_key.as_str());
        cleanup_key(&mut con, leaky_bucket_key.as_str());
    }

    #[test]
    fn test_leaky_bucket_algorithm() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_leaky_bucket";
        let redis_key = build_redis_key_for_suffix(
            bucket_key,
            traffic_policy::PolicyConfig::LeakyBucket {
                capacity: 0,
                period: 0,
            }
            .suffix(),
        );
        cleanup_key(&mut con, redis_key.as_str());

        let remaining_tokens = shield_absorb_with_algorithm(
            &mut con,
            bucket_key,
            5,
            2,
            Some(DEFAULT_TOKENS),
            "leaky_bucket",
        )
        .unwrap();
        assert_eq!(
            remaining_tokens, 4,
            "Leaky bucket should return remaining headroom after accepting a burst"
        );

        let denied_tokens =
            shield_absorb_with_algorithm(&mut con, bucket_key, 5, 2, Some(5), "leaky_bucket")
                .unwrap();
        assert_eq!(
            denied_tokens, -1,
            "Leaky bucket should deny bursts that exceed the configured capacity"
        );

        thread::sleep(time::Duration::from_secs(3));
        let refill_tokens =
            shield_absorb_with_algorithm(&mut con, bucket_key, 5, 2, Some(5), "leaky_bucket")
                .unwrap();
        assert_eq!(
            refill_tokens, 0,
            "Leaky bucket should allow new bursts after the leak period elapses"
        );

        cleanup_key(&mut con, redis_key.as_str());
    }

    #[test]
    fn test_fixed_window_algorithm() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_fixed_window";
        let redis_key = build_redis_key_for_suffix(
            bucket_key,
            traffic_policy::PolicyConfig::FixedWindow {
                capacity: 0,
                period: 0,
            }
            .suffix(),
        );
        cleanup_key(&mut con, redis_key.as_str());

        let remaining_tokens =
            shield_absorb_with_algorithm(&mut con, bucket_key, 3, 1, Some(2), "fixed_window")
                .unwrap();
        assert_eq!(
            remaining_tokens, 1,
            "Fixed window should report remaining capacity within the active window"
        );

        let denied_tokens =
            shield_absorb_with_algorithm(&mut con, bucket_key, 3, 1, Some(2), "fixed_window")
                .unwrap();
        assert_eq!(
            denied_tokens, -1,
            "Fixed window should deny requests that overflow the current window"
        );

        thread::sleep(time::Duration::from_secs(2));
        let reset_tokens =
            shield_absorb_with_algorithm(&mut con, bucket_key, 3, 1, Some(1), "fixed_window")
                .unwrap();
        assert_eq!(
            reset_tokens, 2,
            "After the window expires, capacity should reset for the next window"
        );

        cleanup_key(&mut con, redis_key.as_str());
    }

    #[test]
    fn test_sliding_window_algorithm() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_sliding_window";
        let redis_key = build_redis_key_for_suffix(
            bucket_key,
            traffic_policy::PolicyConfig::SlidingWindow {
                capacity: 0,
                period: 0,
            }
            .suffix(),
        );
        cleanup_key(&mut con, redis_key.as_str());

        let remaining_tokens =
            shield_absorb_with_algorithm(&mut con, bucket_key, 4, 2, Some(3), "sliding_window")
                .unwrap();
        assert_eq!(
            remaining_tokens, 1,
            "Sliding window should track remaining capacity within the current window"
        );

        let denied_tokens =
            shield_absorb_with_algorithm(&mut con, bucket_key, 4, 2, Some(2), "sliding_window")
                .unwrap();
        assert_eq!(
            denied_tokens, -1,
            "Sliding window should deny bursts that exceed the blended window usage"
        );

        thread::sleep(time::Duration::from_secs(2));
        let post_decay_tokens =
            shield_absorb_with_algorithm(&mut con, bucket_key, 4, 2, Some(1), "sliding_window")
                .unwrap();
        assert!(
            post_decay_tokens >= 0,
            "Sliding window should allow new tokens after prior usage decays"
        );

        cleanup_key(&mut con, redis_key.as_str());
    }

    #[test]
    fn test_bucket_does_not_exist() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_new_bucket";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        let remaining_tokens =
            shield_absorb(&mut con, bucket_key, TEST_CAPACITY, TEST_PERIOD, None).unwrap();
        assert_eq!(remaining_tokens, TEST_CAPACITY - DEFAULT_TOKENS);
        let ttl: i64 = con.pttl(redis_key.as_str()).unwrap();
        assert!(
            ttl >= 59900 && ttl <= 60000,
            "TTL should be close to 60000ms, got {}",
            ttl
        );
    }

    #[test]
    fn test_bucket_exists_but_has_no_ttl() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_no_expire";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        // Set a value without TTL
        let _: () = con.set(redis_key.as_str(), 2).unwrap();

        let remaining_tokens =
            shield_absorb(&mut con, bucket_key, TEST_CAPACITY, TEST_PERIOD, None).unwrap();
        assert_eq!(remaining_tokens, TEST_CAPACITY - DEFAULT_TOKENS);

        let ttl: i64 = con.pttl(redis_key.as_str()).unwrap();
        assert!(
            ttl >= 59900 && ttl <= 60000,
            "TTL should be close to 60000ms, got {}",
            ttl
        );
    }

    #[test]
    fn test_multiple_tokens_requested() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_multiple_tokens";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        let tokens_to_consume = 25;
        let remaining_tokens = shield_absorb(
            &mut con,
            bucket_key,
            TEST_CAPACITY,
            TEST_PERIOD,
            Some(tokens_to_consume),
        )
        .unwrap();
        assert_eq!(remaining_tokens, TEST_CAPACITY - tokens_to_consume);
    }

    #[test]
    fn test_bucket_is_overflown() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_overflow";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        let tokens_to_consume = TEST_CAPACITY + 1;
        let remaining_tokens = shield_absorb(
            &mut con,
            bucket_key,
            TEST_CAPACITY,
            TEST_PERIOD,
            Some(tokens_to_consume),
        )
        .unwrap();
        assert_eq!(
            remaining_tokens, -1,
            "Should return -1 when insufficient tokens"
        );
    }

    #[test]
    fn test_sequential_requests() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_sequential";
        let redis_key = build_redis_key(bucket_key);
        let capacity = 2;
        let period = 60;
        cleanup_key(&mut con, redis_key.as_str());

        // First request: should succeed
        let remaining_tokens = shield_absorb(&mut con, bucket_key, capacity, period, None).unwrap();
        assert_eq!(remaining_tokens, 1, "First request should leave 1 token");

        let ttl: i64 = con.pttl(redis_key.as_str()).unwrap();
        assert!(
            ttl >= 59900 && ttl <= 60000,
            "TTL should be close to 60000ms"
        );

        // Second request: should succeed
        let remaining_tokens = shield_absorb(&mut con, bucket_key, capacity, period, None).unwrap();
        assert_eq!(remaining_tokens, 0, "Second request should leave 0 tokens");

        let ttl: i64 = con.pttl(redis_key.as_str()).unwrap();
        assert!(
            ttl >= 59900 && ttl <= 60000,
            "TTL should be close to 60000ms"
        );

        // Third request: should fail (bucket empty)
        let remaining_tokens = shield_absorb(&mut con, bucket_key, capacity, period, None).unwrap();
        assert_eq!(remaining_tokens, -1, "Third request should be denied");

        let ttl: i64 = con.pttl(redis_key.as_str()).unwrap();
        assert!(
            ttl >= 59900 && ttl <= 60000,
            "TTL should be close to 60000ms"
        );
    }

    #[test]
    fn test_bucket_refills_with_time() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_refill";
        let redis_key = build_redis_key(bucket_key);
        let capacity = 3;
        let period = 6;
        cleanup_key(&mut con, redis_key.as_str());

        // Initial request
        let remaining_tokens = shield_absorb(&mut con, bucket_key, capacity, period, None).unwrap();
        assert_eq!(remaining_tokens, 2, "Initial request should leave 2 tokens");

        // Wait for some refill (1/3 of period + buffer)
        thread::sleep(time::Duration::from_secs((period / 3) as u64 + 1));

        // Should have refilled approximately 1 token
        let remaining_tokens = shield_absorb(&mut con, bucket_key, capacity, period, None).unwrap();
        assert_eq!(
            remaining_tokens, 2,
            "After partial refill, should have 2 tokens left"
        );

        // Consume 2 more tokens
        let remaining_tokens =
            shield_absorb(&mut con, bucket_key, capacity, period, Some(2)).unwrap();
        assert_eq!(
            remaining_tokens, 0,
            "After consuming 2 tokens, should have 0 left"
        );

        // Wait for full refill
        thread::sleep(time::Duration::from_secs(period as u64));

        // Should be fully refilled
        let remaining_tokens = shield_absorb(&mut con, bucket_key, capacity, period, None).unwrap();
        assert_eq!(
            remaining_tokens, 2,
            "After full refill, should have 2 tokens left"
        );
    }

    #[test]
    fn test_edge_case_single_token_bucket() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_single_token";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        // Test with capacity=1, period=1
        let remaining_tokens = shield_absorb(&mut con, bucket_key, 1, 1, None).unwrap();
        assert_eq!(
            remaining_tokens, 0,
            "Single token bucket should have 0 tokens left"
        );

        // Second request should fail
        let remaining_tokens = shield_absorb(&mut con, bucket_key, 1, 1, None).unwrap();
        assert_eq!(remaining_tokens, -1, "Second request should be denied");
    }

    #[test]
    fn test_large_capacity_bucket() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_large_capacity";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        let large_capacity = 1000;
        let tokens_to_consume = 500;

        let remaining_tokens = shield_absorb(
            &mut con,
            bucket_key,
            large_capacity,
            TEST_PERIOD,
            Some(tokens_to_consume),
        )
        .unwrap();
        assert_eq!(
            remaining_tokens,
            large_capacity - tokens_to_consume,
            "Large capacity bucket should work correctly"
        );
    }

    #[test]
    fn test_exact_capacity_consumption() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_exact_capacity";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        let capacity = 10;

        // Consume exactly the full capacity
        let remaining_tokens =
            shield_absorb(&mut con, bucket_key, capacity, TEST_PERIOD, Some(capacity)).unwrap();
        assert_eq!(
            remaining_tokens, 0,
            "Consuming exact capacity should leave 0 tokens"
        );

        // Next request should fail
        let remaining_tokens =
            shield_absorb(&mut con, bucket_key, capacity, TEST_PERIOD, None).unwrap();
        assert_eq!(
            remaining_tokens, -1,
            "Request after consuming full capacity should be denied"
        );
    }

    #[test]
    fn test_very_short_period() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_short_period";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        // Test with a very short period (1 second)
        let remaining_tokens = shield_absorb(&mut con, bucket_key, 5, 1, Some(3)).unwrap();
        assert_eq!(
            remaining_tokens, 2,
            "Short period bucket should work correctly"
        );

        // Wait for refill
        thread::sleep(time::Duration::from_secs(2));

        // Should be refilled
        let remaining_tokens = shield_absorb(&mut con, bucket_key, 5, 1, None).unwrap();
        assert_eq!(
            remaining_tokens, 4,
            "After refill, should have 4 tokens left"
        );
    }

    #[test]
    fn test_boundary_conditions() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_boundary";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        // Test consuming capacity - 1 tokens
        let capacity = 100;
        let tokens_to_consume = capacity - 1;

        let remaining_tokens = shield_absorb(
            &mut con,
            bucket_key,
            capacity,
            TEST_PERIOD,
            Some(tokens_to_consume),
        )
        .unwrap();
        assert_eq!(remaining_tokens, 1, "Should have exactly 1 token left");

        // Consume the last token
        let remaining_tokens =
            shield_absorb(&mut con, bucket_key, capacity, TEST_PERIOD, None).unwrap();
        assert_eq!(remaining_tokens, 0, "Should have exactly 0 tokens left");

        // Next request should fail
        let remaining_tokens =
            shield_absorb(&mut con, bucket_key, capacity, TEST_PERIOD, None).unwrap();
        assert_eq!(
            remaining_tokens, -1,
            "Request with empty bucket should be denied"
        );
    }

    // Missing test cases for better coverage

    #[test]
    #[should_panic(expected = "ResponseError: invalid token count in Redis")]
    fn test_corrupted_redis_data() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_corrupted_data";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());
        // Set invalid (non-numeric) data in Redis
        let _: () = con.set(redis_key.as_str(), "corrupted_data").unwrap();
        println!("Set corrupted data in key: {}", bucket_key);
        // Should detect corrupted data and fail fast for security
        let _ = shield_absorb(&mut con, bucket_key, 10, TEST_PERIOD, None).unwrap();
    }

    #[test]
    #[should_panic(
        expected = "\"WRONGTYPE\": Operation against a key holding the wrong kind of value"
    )]
    fn test_redis_key_with_different_data_types() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_different_types";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        // Test with different Redis value types
        let _: () = con.hset(redis_key.as_str(), "field", "value").unwrap();

        // Should detect wrong Redis data type and fail fast
        let _ = shield_absorb(&mut con, bucket_key, 10, TEST_PERIOD, None).unwrap();
    }

    #[test]
    fn test_maximum_values() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_max_values";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        // Test with very large values (within reasonable bounds)
        let large_capacity = 10000;
        let large_period = 3600; // 1 hour
        let large_tokens = 5000;

        let remaining_tokens = shield_absorb(
            &mut con,
            bucket_key,
            large_capacity,
            large_period,
            Some(large_tokens),
        )
        .unwrap();
        assert_eq!(remaining_tokens, large_capacity - large_tokens);
    }

    #[test]
    fn test_concurrent_buckets() {
        let mut con = establish_connection();
        let bucket_key1 = "redis-shield::test_concurrent_1";
        let bucket_key2 = "redis-shield::test_concurrent_2";
        let redis_key1 = build_redis_key(bucket_key1);
        let redis_key2 = build_redis_key(bucket_key2);
        cleanup_key(&mut con, redis_key1.as_str());
        cleanup_key(&mut con, redis_key2.as_str());

        // Test that different buckets don't interfere with each other
        let tokens1 = shield_absorb(&mut con, bucket_key1, 10, TEST_PERIOD, Some(5)).unwrap();
        let tokens2 = shield_absorb(&mut con, bucket_key2, 20, TEST_PERIOD, Some(8)).unwrap();

        assert_eq!(tokens1, 5, "First bucket should have 5 tokens left");
        assert_eq!(tokens2, 12, "Second bucket should have 12 tokens left");

        // Verify they remain independent
        let tokens1_again = shield_absorb(&mut con, bucket_key1, 10, TEST_PERIOD, Some(2)).unwrap();
        assert_eq!(tokens1_again, 3, "First bucket should now have 3 tokens");
    }

    #[test]
    fn test_bucket_key_edge_cases() {
        let mut con = establish_connection();

        // Test with special characters in key
        let special_key = "redis-shield::test:with:colons:and-dashes_and_underscores";
        let redis_key = build_redis_key(special_key);
        cleanup_key(&mut con, redis_key.as_str());

        let remaining_tokens = shield_absorb(&mut con, special_key, 5, TEST_PERIOD, None).unwrap();
        assert_eq!(
            remaining_tokens, 4,
            "Should handle special characters in key"
        );

        // Test with empty-like key (but not actually empty)
        let minimal_key = "x";
        let redis_key_minimal = build_redis_key(minimal_key);
        cleanup_key(&mut con, redis_key_minimal.as_str());

        let remaining_tokens = shield_absorb(&mut con, minimal_key, 5, TEST_PERIOD, None).unwrap();
        assert_eq!(remaining_tokens, 4, "Should handle minimal key");
    }

    #[test]
    fn test_refill_precision() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_refill_precision";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        let capacity = 100;
        let period = 10; // 10 seconds

        // Consume some tokens
        let remaining_tokens =
            shield_absorb(&mut con, bucket_key, capacity, period, Some(50)).unwrap();
        assert_eq!(remaining_tokens, 50);

        // Wait for half the period
        thread::sleep(time::Duration::from_secs(5));

        // Should have refilled approximately 50 tokens (half the capacity over half the period)
        let remaining_tokens = shield_absorb(&mut con, bucket_key, capacity, period, None).unwrap();
        // Allow for some timing variance
        assert!(
            remaining_tokens >= 98 && remaining_tokens <= 99,
            "Should have refilled approximately 50 tokens, got {}",
            remaining_tokens
        );
    }

    #[test]
    fn test_ttl_edge_cases() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_ttl_edge_cases";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        // Create bucket and verify TTL is set correctly
        let _remaining_tokens = shield_absorb(&mut con, bucket_key, 10, 30, None).unwrap();

        let ttl: i64 = con.pttl(redis_key.as_str()).unwrap();
        assert!(
            ttl >= 29900 && ttl <= 30000,
            "TTL should be close to 30000ms for 30s period, got {}",
            ttl
        );

        // Test with very short period
        let short_key = "redis-shield::test_ttl_short";
        let redis_short_key = build_redis_key(short_key);
        cleanup_key(&mut con, redis_short_key.as_str());

        let _remaining_tokens = shield_absorb(&mut con, short_key, 10, 1, None).unwrap();
        let ttl: i64 = con.pttl(redis_short_key.as_str()).unwrap();
        assert!(
            ttl >= 900 && ttl <= 1000,
            "TTL should be close to 1000ms for 1s period, got {}",
            ttl
        );
    }

    #[test]
    fn test_zero_tokens_consumption() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_zero_consumption";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        // Test default tokens consumption (should be 1)
        let remaining_tokens = shield_absorb(&mut con, bucket_key, 10, TEST_PERIOD, None).unwrap();
        assert_eq!(remaining_tokens, 9, "Default token consumption should be 1");

        // Verify the bucket state is consistent
        let remaining_tokens = shield_absorb(&mut con, bucket_key, 10, TEST_PERIOD, None).unwrap();
        assert_eq!(
            remaining_tokens, 8,
            "Should continue consuming 1 token by default"
        );
    }

    #[test]
    fn test_redis_connection_resilience() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_redis_resilience";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        // Test multiple operations to ensure Redis connection stability
        for i in 0..5 {
            let expected_tokens = 9 - i;
            let remaining_tokens =
                shield_absorb(&mut con, bucket_key, 10, TEST_PERIOD, None).unwrap();
            assert_eq!(
                remaining_tokens,
                expected_tokens,
                "Operation {} should leave {} tokens",
                i + 1,
                expected_tokens
            );
        }
    }

    #[test]
    #[should_panic(expected = "ResponseError: period value too large")]
    fn test_period_overflow() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_period_overflow";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        // Period that would overflow when multiplied by 1000
        // i64::MAX / 1000 = 9,223,372,036,854,775
        // Any value larger than this should trigger overflow protection
        let overflow_period = i64::MAX / 1000 + 1;

        let _ = shield_absorb(&mut con, bucket_key, 10, overflow_period, None).unwrap();
    }

    #[test]
    fn test_maximum_safe_period() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_max_safe_period";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        // Use a very large period that tests overflow protection without exceeding Redis limits
        // Redis internally converts TTL to absolute timestamp (current_time_ms + ttl_ms)
        // Using 100 years = 3,153,600,000 seconds is large enough to test our overflow checks
        // while staying well within Redis's timestamp limits (which go to year 9999+)
        let max_safe_period = 3_153_600_000; // 100 years in seconds

        let remaining_tokens =
            shield_absorb(&mut con, bucket_key, 10, max_safe_period, None).unwrap();
        assert_eq!(
            remaining_tokens, 9,
            "Maximum safe period should work correctly"
        );
    }

    #[test]
    fn test_large_capacity_no_overflow() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_large_capacity_no_overflow";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        // Use a very large capacity to test saturating_add behavior
        // We can't use i64::MAX because capacity * elapsed_fraction might overflow in f64
        // But we can use a large enough value to verify tokens don't go negative
        let large_capacity = i64::MAX / 2;
        let tokens_to_consume = 1000;

        let remaining_tokens = shield_absorb(
            &mut con,
            bucket_key,
            large_capacity,
            TEST_PERIOD,
            Some(tokens_to_consume),
        )
        .unwrap();

        // Verify tokens are positive and correctly calculated
        assert!(
            remaining_tokens >= 0,
            "Tokens should never be negative, got {}",
            remaining_tokens
        );
        assert_eq!(
            remaining_tokens,
            large_capacity - tokens_to_consume,
            "Token calculation should be correct"
        );
    }

    #[test]
    fn test_extreme_capacity_values() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_extreme_capacity";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        // Test with extremely large capacity
        let extreme_capacity = 1_000_000_000_000_i64; // 1 trillion

        // Create bucket and consume some tokens
        let remaining_tokens =
            shield_absorb(&mut con, bucket_key, extreme_capacity, 60, Some(1000)).unwrap();

        assert!(
            remaining_tokens >= 0,
            "Tokens should never be negative with extreme capacity"
        );
        assert_eq!(
            remaining_tokens,
            extreme_capacity - 1000,
            "Should handle extreme capacity correctly"
        );

        // Verify multiple operations work correctly
        // Note: With large capacity and 60s period, even microseconds of elapsed time
        // will refill tokens. So we check range instead of exact value.
        let remaining_tokens =
            shield_absorb(&mut con, bucket_key, extreme_capacity, 60, Some(5000)).unwrap();

        assert!(remaining_tokens >= 0, "Tokens should remain positive");
        // Should be close to (extreme_capacity - 1000 - 5000), allowing for refill
        let expected_min = extreme_capacity - 1000 - 5000;
        let expected_max = extreme_capacity - 5000; // Max if fully refilled before second call
        assert!(
            remaining_tokens >= expected_min && remaining_tokens <= expected_max,
            "Sequential operations should work correctly, expected between {} and {}, got {}",
            expected_min,
            expected_max,
            remaining_tokens
        );
    }

    #[test]
    fn test_token_refill_with_large_values() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_refill_large_values";
        let redis_key = build_redis_key(bucket_key);
        cleanup_key(&mut con, redis_key.as_str());

        // Use a large capacity with a short period to test refill calculation
        let large_capacity = 10_000_000_i64;
        let short_period = 2; // 2 seconds

        // Consume some tokens
        let remaining_tokens = shield_absorb(
            &mut con,
            bucket_key,
            large_capacity,
            short_period,
            Some(5_000_000),
        )
        .unwrap();
        assert_eq!(
            remaining_tokens,
            large_capacity - 5_000_000,
            "Initial consumption should be correct"
        );

        // Wait for refill
        thread::sleep(time::Duration::from_secs(short_period as u64 + 1));

        // Should be fully refilled (or very close)
        let remaining_tokens = shield_absorb(
            &mut con,
            bucket_key,
            large_capacity,
            short_period,
            Some(1_000_000),
        )
        .unwrap();

        // After full refill and consuming 1M, should have large_capacity - 1M
        assert!(
            remaining_tokens >= large_capacity - 1_000_000 - 100_000,
            "Should refill correctly with large capacity, got {}",
            remaining_tokens
        );
        assert!(
            remaining_tokens >= 0,
            "Tokens should never be negative after refill"
        );
    }
}
