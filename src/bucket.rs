use num::clamp;
use redis_module::{Context, RedisError, RedisString, RedisValue};
use std::cmp::min;

const MILLIS_IN_SEC: i64 = 1000;
const MIN_TTL: i64 = 0;
const MIN_TOKENS: i64 = 0;
const INSUFFICIENT_TOKENS: i64 = -1;

/// The token bucket algorithm is based on an analogy of a fixed capacity bucket
/// into which tokens are added at a fixed rate. When a request is to be checked
/// for conformance to the defined limits, the bucket is inspected to see if it
/// contains sufficient tokens at that time. If so, the appropriate number of tokens,
/// e.g. equivalent to the number of HTTP requests, are removed,
/// and the request is passed.
///
/// The request does not conform if there are insufficient tokens in the bucket,
/// and the contents of the bucket are not changed.
pub struct Bucket<'a> {
    // Unique bucket key used to store its details in redis
    pub key: &'a RedisString,
    // Maximum bucket's capacity
    pub capacity: i64,
    // Replenish period in which `capacity` number of tokens is refilled
    pub period: i64,
    // Number of tokens left in the bucket. When a bucket is created, `tokens = capacity`
    pub tokens: i64,
    // Redis context used to perform redis commands
    ctx: &'a Context,
}

impl<'a> Bucket<'a> {
    /// Instantiates a new bucket.
    ///
    /// If the key already exists in redis:
    ///     * Fetches info about tokens left and TTL
    ///     * Sanitizes the fetched numbers
    ///     * Adds tokens refilled since the last request.
    ///
    /// # Arguments
    /// * `ctx` - Redis context for executing commands
    /// * `key` - Unique bucket identifier
    /// * `capacity` - Maximum number of tokens (must be positive)
    /// * `period` - Replenishment period in seconds (must be positive)
    ///
    /// # Errors
    /// Returns `RedisError` if:
    /// - `capacity` or `period` are not positive
    /// - Redis operations fail
    pub fn new(
        ctx: &'a Context,
        key: &'a RedisString,
        capacity: i64,
        period: i64,
    ) -> Result<Self, RedisError> {
        if capacity <= 0 {
            return Err(RedisError::String(
                "ERR capacity must be positive".to_string(),
            ));
        }
        if period <= 0 {
            return Err(RedisError::String(
                "ERR period must be positive".to_string(),
            ));
        }

        let mut bucket = Self {
            ctx,
            key,
            capacity,
            period: period * MILLIS_IN_SEC,
            tokens: MIN_TOKENS,
        };
        bucket.fetch_tokens()?;
        Ok(bucket)
    }

    /// Attempts to remove requested number of `tokens` from the bucket.
    ///
    /// If the bucket doesn't contain sufficient tokens, no tokens are
    /// removed and `-1` is returned.
    ///
    /// If the bucket contains enough tokens, `tokens` are removed from the bucket,
    /// and the number of tokens left is returned.
    ///
    /// # Arguments
    /// * `tokens` - Number of tokens to remove (must be positive)
    ///
    /// # Returns
    /// * `Ok(tokens_left)` - Number of tokens remaining after removal
    /// * `Ok(-1)` - If insufficient tokens available
    /// * `Err(RedisError)` - If Redis operations fail or invalid input
    pub fn pour(&mut self, tokens: i64) -> Result<i64, RedisError> {
        if tokens <= 0 {
            return Err(RedisError::String(
                "ERR tokens must be positive".to_string(),
            ));
        }

        if tokens > self.tokens {
            Ok(INSUFFICIENT_TOKENS)
        } else {
            self.tokens -= tokens;

            // Pre-create strings to avoid repeated allocations in Redis call
            let period_str = self.period.to_string();
            let tokens_str = self.tokens.to_string();

            self.ctx.call(
                "PSETEX",
                &[
                    self.key,
                    &RedisString::create(None, period_str.as_str()),
                    &RedisString::create(None, tokens_str.as_str()),
                ],
            )?;
            Ok(self.tokens)
        }
    }

    /// Fetches the current token count and calculates refilled tokens based on elapsed time.
    ///
    /// This method:
    /// 1. Gets the current TTL of the bucket key
    /// 2. Calculates how much time has passed since the last update
    /// 3. Calculates how many tokens should be refilled based on elapsed time
    /// 4. Updates the bucket's token count, capped at the maximum capacity
    fn fetch_tokens(&mut self) -> Result<(), RedisError> {
        // Starting with Redis 2.8 the return value of PTTL in case of error changed:
        //     - The command returns -2 if the key does not exist.
        //     - The command returns -1 if the key exists but has no associated expire.
        let current_ttl = match self.ctx.call("PTTL", &[self.key])? {
            RedisValue::Integer(ttl) => clamp(ttl, MIN_TTL, self.period),
            _ => MIN_TTL,
        };

        // Calculate the fraction of the period that has elapsed
        let elapsed_fraction = if self.period > 0 {
            (self.period - current_ttl) as f64 / self.period as f64
        } else {
            0.0
        };

        // Calculate how many tokens should be refilled based on elapsed time
        let refilled_tokens = (elapsed_fraction * self.capacity as f64) as i64;

        // Get the current token count stored in Redis
        let remaining_tokens = match self.ctx.call("GET", &[self.key])? {
            RedisValue::SimpleString(tokens_str) => tokens_str
                .parse::<i64>()
                .map_err(|_| RedisError::String("ERR invalid token count in Redis".to_string()))?
                .max(MIN_TOKENS),
            _ => MIN_TOKENS,
        };

        // Update token count: add refilled tokens but don't exceed capacity
        self.tokens = min(self.capacity, remaining_tokens + refilled_tokens);
        Ok(())
    }
}
