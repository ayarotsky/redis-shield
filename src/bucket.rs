use redis_module::{Context, RedisError, RedisString, RedisValue};

const MILLIS_IN_SEC: i64 = 1000;
const MIN_TTL: i64 = 0;
const MIN_TOKENS: i64 = 0;
const INSUFFICIENT_TOKENS: i64 = -1;

// Pre-allocated error messages (static strings to avoid repeated allocations)
const ERR_CAPACITY_POSITIVE: &str = "ERR capacity must be positive";
const ERR_PERIOD_POSITIVE: &str = "ERR period must be positive";
const ERR_PERIOD_TOO_LARGE: &str = "ERR period value too large";
const ERR_TOKENS_POSITIVE: &str = "ERR tokens must be positive";
const ERR_INVALID_TOKEN_COUNT: &str = "ERR invalid token count in Redis";

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
    #[inline]
    pub fn new(
        ctx: &'a Context,
        key: &'a RedisString,
        capacity: i64,
        period: i64,
    ) -> Result<Self, RedisError> {
        if capacity <= 0 {
            return Err(RedisError::String(ERR_CAPACITY_POSITIVE.into()));
        }
        if period <= 0 {
            return Err(RedisError::String(ERR_PERIOD_POSITIVE.into()));
        }

        let period_ms = period
            .checked_mul(MILLIS_IN_SEC)
            .ok_or(RedisError::String(ERR_PERIOD_TOO_LARGE.into()))?;

        let mut bucket = Self {
            ctx,
            key,
            capacity,
            period: period_ms,
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
    #[inline]
    pub fn pour(&mut self, tokens: i64) -> Result<i64, RedisError> {
        if tokens <= 0 {
            return Err(RedisError::String(ERR_TOKENS_POSITIVE.into()));
        }

        if tokens > self.tokens {
            Ok(INSUFFICIENT_TOKENS)
        } else {
            self.tokens -= tokens;

            // Use itoa for fast, zero-allocation integer-to-string conversion
            // This avoids heap allocations by using stack buffers
            let mut period_buf = itoa::Buffer::new();
            let mut tokens_buf = itoa::Buffer::new();
            let period_str = period_buf.format(self.period);
            let tokens_str = tokens_buf.format(self.tokens);

            self.ctx.call(
                "PSETEX",
                &[
                    self.key,
                    &RedisString::create(None, period_str),
                    &RedisString::create(None, tokens_str),
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
    #[inline]
    fn fetch_tokens(&mut self) -> Result<(), RedisError> {
        // Starting with Redis 2.8 the return value of PTTL in case of error changed:
        //     - The command returns -2 if the key does not exist.
        //     - The command returns -1 if the key exists but has no associated expire.
        let current_ttl = match self.ctx.call("PTTL", &[self.key])? {
            RedisValue::Integer(ttl) => ttl.clamp(MIN_TTL, self.period),
            _ => MIN_TTL,
        };

        // Calculate how many tokens should be refilled based on elapsed time
        // Use integer arithmetic to avoid float conversion overhead
        // We use i128 for intermediate calculation to prevent overflow
        let elapsed = self.period - current_ttl;
        let refilled_tokens = ((elapsed as i128 * self.capacity as i128) / self.period as i128) as i64;

        // Get the current token count stored in Redis
        let remaining_tokens = match self.ctx.call("GET", &[self.key])? {
            RedisValue::SimpleString(tokens_str) => tokens_str
                .parse::<i64>()
                .map_err(|_| RedisError::String(ERR_INVALID_TOKEN_COUNT.into()))?
                .max(MIN_TOKENS),
            _ => MIN_TOKENS,
        };

        // Update token count: add refilled tokens but don't exceed capacity
        // Use saturating_add to prevent overflow before min() is applied
        self.tokens = remaining_tokens.saturating_add(refilled_tokens).min(self.capacity);
        Ok(())
    }
}
