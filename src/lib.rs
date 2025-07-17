mod bucket;

use bucket::Bucket;
use redis_module::{redis_module, Context, RedisError, RedisResult, RedisString};

// Command argument constraints
const MIN_ARGS_LEN: usize = 4;
const MAX_ARGS_LEN: usize = 5;

// Default values
const DEFAULT_TOKENS: i64 = 1;

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
fn redis_command(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    // Validate argument count
    if !(MIN_ARGS_LEN..=MAX_ARGS_LEN).contains(&args.len()) {
        return Err(RedisError::WrongArity);
    }

    // Parse and validate arguments
    let capacity = parse_positive_integer("capacity", &args[2])?;
    let period = parse_positive_integer("period", &args[3])?;
    let tokens = match args.len() {
        MAX_ARGS_LEN => parse_positive_integer("tokens", &args[4])?,
        _ => DEFAULT_TOKENS,
    };

    // Create bucket and attempt to consume tokens
    let mut bucket = Bucket::new(ctx, &args[1], capacity, period)?;
    let remaining_tokens = bucket.pour(tokens)?;

    Ok(remaining_tokens.into())
}

/// Parses a RedisString argument as a positive integer.
///
/// # Arguments
/// * `name` - The name of the parameter for error messages
/// * `value` - The RedisString value to parse
///
/// # Returns
/// * `Ok(i64)` - The parsed positive integer
/// * `Err(RedisError)` - If the value is not a positive integer
///
/// # Errors
/// Returns a RedisError with a descriptive message if:
/// - The value cannot be parsed as an integer
/// - The parsed integer is not positive (≤ 0)
fn parse_positive_integer(name: &str, value: &RedisString) -> Result<i64, RedisError> {
    match value.parse_integer() {
        Ok(arg) if arg > 0 => Ok(arg),
        _ => Err(RedisError::String(format!("ERR {} must be positive", name))),
    }
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
        let mut cmd = redis::cmd(REDIS_COMMAND);
        cmd.arg(key).arg(capacity).arg(period);
        if let Some(t) = tokens {
            cmd.arg(t);
        }
        cmd.query(con)
    }

    /// Helper function to clean up test keys.
    fn cleanup_key(con: &mut redis::Connection, key: &str) {
        let _: () = con.del(key).unwrap();
    }

    // Test constants for better readability
    const TEST_CAPACITY: i64 = 30;
    const TEST_PERIOD: i64 = 60;

    #[test]
    #[should_panic(
        expected = "An error was signalled by the server - ResponseError: wrong number of arguments for 'SHIELD.absorb' command"
    )]
    fn test_wrong_arity() {
        let mut con = establish_connection();
        let _: () = redis::cmd(REDIS_COMMAND).query(&mut con).unwrap();
    }

    #[test]
    #[should_panic(
        expected = "An error was signalled by the server - ResponseError: capacity must be positive"
    )]
    fn test_capacity_is_string() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_capacity_string";
        cleanup_key(&mut con, bucket_key);

        let _: () = redis::cmd(REDIS_COMMAND)
            .arg(bucket_key)
            .arg("abc")
            .arg(TEST_PERIOD)
            .query(&mut con)
            .unwrap();
    }

    #[test]
    #[should_panic(
        expected = "An error was signalled by the server - ResponseError: capacity must be positive"
    )]
    fn test_capacity_is_float() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_capacity_float";
        cleanup_key(&mut con, bucket_key);

        let _: () = redis::cmd(REDIS_COMMAND)
            .arg(bucket_key)
            .arg(1.2)
            .arg(TEST_PERIOD)
            .query(&mut con)
            .unwrap();
    }

    #[test]
    #[should_panic(
        expected = "An error was signalled by the server - ResponseError: capacity must be positive"
    )]
    fn test_capacity_is_zero() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_capacity_zero";
        cleanup_key(&mut con, bucket_key);

        let _ = shield_absorb(&mut con, bucket_key, 0, TEST_PERIOD, None).unwrap();
    }

    #[test]
    #[should_panic(
        expected = "An error was signalled by the server - ResponseError: capacity must be positive"
    )]
    fn test_capacity_is_negative() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_capacity_negative";
        cleanup_key(&mut con, bucket_key);

        let _ = shield_absorb(&mut con, bucket_key, -2, TEST_PERIOD, None).unwrap();
    }

    #[test]
    #[should_panic(
        expected = "An error was signalled by the server - ResponseError: period must be positive"
    )]
    fn test_period_is_string() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_period_string";
        cleanup_key(&mut con, bucket_key);

        let _: () = redis::cmd(REDIS_COMMAND)
            .arg(bucket_key)
            .arg(TEST_CAPACITY)
            .arg("abc")
            .query(&mut con)
            .unwrap();
    }

    #[test]
    #[should_panic(
        expected = "An error was signalled by the server - ResponseError: period must be positive"
    )]
    fn test_period_is_float() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_period_float";
        cleanup_key(&mut con, bucket_key);

        let _: () = redis::cmd(REDIS_COMMAND)
            .arg(bucket_key)
            .arg(TEST_CAPACITY)
            .arg(3.14)
            .query(&mut con)
            .unwrap();
    }

    #[test]
    #[should_panic(
        expected = "An error was signalled by the server - ResponseError: period must be positive"
    )]
    fn test_period_is_zero() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_period_zero";
        cleanup_key(&mut con, bucket_key);

        let _ = shield_absorb(&mut con, bucket_key, TEST_CAPACITY, 0, None).unwrap();
    }

    #[test]
    #[should_panic(
        expected = "An error was signalled by the server - ResponseError: period must be positive"
    )]
    fn test_period_is_negative() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_period_negative";
        cleanup_key(&mut con, bucket_key);

        let _ = shield_absorb(&mut con, bucket_key, TEST_CAPACITY, -4, None).unwrap();
    }

    #[test]
    #[should_panic(
        expected = "An error was signalled by the server - ResponseError: tokens must be positive"
    )]
    fn test_tokens_is_string() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_tokens_string";
        cleanup_key(&mut con, bucket_key);

        let _: () = redis::cmd(REDIS_COMMAND)
            .arg(bucket_key)
            .arg(TEST_CAPACITY)
            .arg(TEST_PERIOD)
            .arg("abc")
            .query(&mut con)
            .unwrap();
    }

    #[test]
    #[should_panic(
        expected = "An error was signalled by the server - ResponseError: tokens must be positive"
    )]
    fn test_tokens_is_float() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_tokens_float";
        cleanup_key(&mut con, bucket_key);

        let _: () = redis::cmd(REDIS_COMMAND)
            .arg(bucket_key)
            .arg(TEST_CAPACITY)
            .arg(TEST_PERIOD)
            .arg(2.5)
            .query(&mut con)
            .unwrap();
    }

    #[test]
    #[should_panic(
        expected = "An error was signalled by the server - ResponseError: tokens must be positive"
    )]
    fn test_tokens_is_zero() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_tokens_zero";
        cleanup_key(&mut con, bucket_key);

        let _ = shield_absorb(&mut con, bucket_key, TEST_CAPACITY, TEST_PERIOD, Some(0)).unwrap();
    }

    #[test]
    #[should_panic(
        expected = "An error was signalled by the server - ResponseError: tokens must be positive"
    )]
    fn test_tokens_is_negative() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_tokens_negative";
        cleanup_key(&mut con, bucket_key);

        let _ = shield_absorb(&mut con, bucket_key, TEST_CAPACITY, TEST_PERIOD, Some(-9)).unwrap();
    }

    #[test]
    fn test_bucket_does_not_exist() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_new_bucket";
        cleanup_key(&mut con, bucket_key);

        let remaining_tokens =
            shield_absorb(&mut con, bucket_key, TEST_CAPACITY, TEST_PERIOD, None).unwrap();
        assert_eq!(remaining_tokens, TEST_CAPACITY - DEFAULT_TOKENS);

        let ttl: i64 = con.pttl(bucket_key).unwrap();
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
        cleanup_key(&mut con, bucket_key);

        // Set a value without TTL
        let _: () = con.set(bucket_key, 2).unwrap();

        let remaining_tokens =
            shield_absorb(&mut con, bucket_key, TEST_CAPACITY, TEST_PERIOD, None).unwrap();
        assert_eq!(remaining_tokens, TEST_CAPACITY - DEFAULT_TOKENS);

        let ttl: i64 = con.pttl(bucket_key).unwrap();
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
        cleanup_key(&mut con, bucket_key);

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
        cleanup_key(&mut con, bucket_key);

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
        let capacity = 2;
        let period = 60;
        cleanup_key(&mut con, bucket_key);

        // First request: should succeed
        let remaining_tokens = shield_absorb(&mut con, bucket_key, capacity, period, None).unwrap();
        assert_eq!(remaining_tokens, 1, "First request should leave 1 token");

        let ttl: i64 = con.pttl(bucket_key).unwrap();
        assert!(
            ttl >= 59900 && ttl <= 60000,
            "TTL should be close to 60000ms"
        );

        // Second request: should succeed
        let remaining_tokens = shield_absorb(&mut con, bucket_key, capacity, period, None).unwrap();
        assert_eq!(remaining_tokens, 0, "Second request should leave 0 tokens");

        let ttl: i64 = con.pttl(bucket_key).unwrap();
        assert!(
            ttl >= 59900 && ttl <= 60000,
            "TTL should be close to 60000ms"
        );

        // Third request: should fail (bucket empty)
        let remaining_tokens = shield_absorb(&mut con, bucket_key, capacity, period, None).unwrap();
        assert_eq!(remaining_tokens, -1, "Third request should be denied");

        let ttl: i64 = con.pttl(bucket_key).unwrap();
        assert!(
            ttl >= 59900 && ttl <= 60000,
            "TTL should be close to 60000ms"
        );
    }

    #[test]
    fn test_bucket_refills_with_time() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_refill";
        let capacity = 3;
        let period = 6;
        cleanup_key(&mut con, bucket_key);

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
        cleanup_key(&mut con, bucket_key);

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
        cleanup_key(&mut con, bucket_key);

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
        cleanup_key(&mut con, bucket_key);

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
        cleanup_key(&mut con, bucket_key);

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
        cleanup_key(&mut con, bucket_key);

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
    #[should_panic(
        expected = "An error was signalled by the server - ResponseError: invalid token count in Redis"
    )]
    fn test_corrupted_redis_data() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_corrupted_data";
        cleanup_key(&mut con, bucket_key);

        // Set invalid (non-numeric) data in Redis
        let _: () = con.set(bucket_key, "corrupted_data").unwrap();

        // Should detect corrupted data and fail fast for security
        let _ = shield_absorb(&mut con, bucket_key, 10, TEST_PERIOD, None).unwrap();
    }

    #[test]
    #[should_panic(expected = "WRONGTYPE: Operation against a key holding the wrong kind of value")]
    fn test_redis_key_with_different_data_types() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_different_types";
        cleanup_key(&mut con, bucket_key);

        // Test with different Redis value types
        let _: () = con.hset(bucket_key, "field", "value").unwrap();

        // Should detect wrong Redis data type and fail fast
        let _ = shield_absorb(&mut con, bucket_key, 10, TEST_PERIOD, None).unwrap();
    }

    #[test]
    fn test_maximum_values() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_max_values";
        cleanup_key(&mut con, bucket_key);

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
        cleanup_key(&mut con, bucket_key1);
        cleanup_key(&mut con, bucket_key2);

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
        cleanup_key(&mut con, special_key);

        let remaining_tokens = shield_absorb(&mut con, special_key, 5, TEST_PERIOD, None).unwrap();
        assert_eq!(
            remaining_tokens, 4,
            "Should handle special characters in key"
        );

        // Test with empty-like key (but not actually empty)
        let minimal_key = "x";
        cleanup_key(&mut con, minimal_key);

        let remaining_tokens = shield_absorb(&mut con, minimal_key, 5, TEST_PERIOD, None).unwrap();
        assert_eq!(remaining_tokens, 4, "Should handle minimal key");
    }

    #[test]
    fn test_refill_precision() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_refill_precision";
        cleanup_key(&mut con, bucket_key);

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
        cleanup_key(&mut con, bucket_key);

        // Create bucket and verify TTL is set correctly
        let _remaining_tokens = shield_absorb(&mut con, bucket_key, 10, 30, None).unwrap();

        let ttl: i64 = con.pttl(bucket_key).unwrap();
        assert!(
            ttl >= 29900 && ttl <= 30000,
            "TTL should be close to 30000ms for 30s period, got {}",
            ttl
        );

        // Test with very short period
        let short_key = "redis-shield::test_ttl_short";
        cleanup_key(&mut con, short_key);

        let _remaining_tokens = shield_absorb(&mut con, short_key, 10, 1, None).unwrap();
        let ttl: i64 = con.pttl(short_key).unwrap();
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
        cleanup_key(&mut con, bucket_key);

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
        cleanup_key(&mut con, bucket_key);

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
}
