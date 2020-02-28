use num::clamp;
use redis_module::{parse_integer, Context, RedisError, RedisValue};
use std::cmp::{max, min};

const MILLS_IN_SEC: i64 = 1000;
const MIN_TTL: i64 = 0;
const MIN_TOKENS: i64 = 0;
const OVERFLOWN_RESPONSE: i64 = -1;

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
    pub key: &'a str,
    // Maximum bucket's capacity
    pub capacity: i64,
    // Replenish period in which `capacity` number of tokens is refilled
    pub period: i64,
    // Number of tokens left in the bucket. When a bucket is created, `tokens = capacity`
    pub tokens: i64,
    // Redis context used to perform perform redis commands
    ctx: &'a Context,
}

impl<'a> Bucket<'a> {
    /// Instantiates a new bucket.
    ///
    /// If the key already exists in redis:
    ///     * Fetches info about tokens left and TTL
    ///     * Sanitizes the fetched numbers
    ///     * Adds tokens tokens refilled since the last request.
    pub fn new(
        ctx: &'a Context,
        key: &'a str,
        capacity: i64,
        period: i64,
    ) -> Result<Self, RedisError> {
        let mut bucket = Self {
            ctx,
            key,
            capacity,
            period: period * MILLS_IN_SEC,
            tokens: MIN_TOKENS,
        };
        bucket.fetch_tokens()?;
        Ok(bucket)
    }

    /// Attempts to remove requested number of `tokens` from the bucket.
    ///
    /// If the bucket doesn't contain sufficient tokens, no tokens are
    /// remove and `-1` is returned.
    ///
    /// If the bucket contains enough tokens, `tokens` are removed from the bucket,
    /// and the number of tokens left is returned.
    pub fn pour(&mut self, tokens: i64) -> Result<i64, RedisError> {
        if tokens > self.tokens {
            Ok(OVERFLOWN_RESPONSE)
        } else {
            self.tokens -= tokens;
            self.ctx.call(
                "PSETEX",
                &[self.key, &self.period.to_string(), &self.tokens.to_string()],
            )?;
            Ok(self.tokens)
        }
    }

    fn fetch_tokens(&mut self) -> Result<(), RedisError> {
        // Starting with Redis 2.8 the return value of PTTL in case of error changed:
        //     - The command returns -2 if the key does not exist.
        //     - The command returns -1 if the key exists but has no associated expire.
        let current_ttl = match self.ctx.call("PTTL", &[self.key])? {
            RedisValue::Integer(ttl) => clamp(ttl, MIN_TTL, self.period),
            _ => MIN_TTL,
        };
        let delta = (self.period - current_ttl) as f64 / self.period as f64;
        let refilled_tokens = (delta * self.capacity as f64) as i64;
        let remaining_tokens = match self.ctx.call("GET", &[self.key])? {
            RedisValue::SimpleString(tokens) => max(MIN_TOKENS, parse_integer(&tokens)?),
            _ => MIN_TOKENS,
        };

        self.tokens = min(self.capacity, remaining_tokens + refilled_tokens);
        Ok(())
    }
}
