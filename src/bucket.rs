use num::clamp;
use redis_module::{parse_integer, Context, RedisError, RedisValue};
use std::cmp::{max, min};

const MILLS_IN_SEC: i64 = 1000;
const MIN_TTL: i64 = 0;
const MIN_TOKENS: i64 = 0;

pub struct Bucket<'a> {
    pub key: &'a str,
    pub capacity: i64,
    pub period: i64,
    pub tokens: i64,
    ctx: &'a Context,
}

impl<'a> Bucket<'a> {
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

    pub fn pour(&mut self, tokens: i64) -> Result<i64, RedisError> {
        if tokens > self.tokens {
            Err(RedisError::Str("ERR bucket is overflown"))
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
