use redis_module::{Context, RedisError, RedisValue, parse_integer};
use std::cmp::{min, max};

const MAX_TTL: i64 = 1000;
const MIN_TTL: i64 = 0;
const MIN_TOKENS: i64 = 0;

pub struct Bucket<'a> {
    pub key: &'a str,
    pub capacity: i64,
    pub tokens: i64,
    ctx: &'a Context,
}

impl<'a> Bucket<'a> {
    pub fn new(ctx: &'a Context, key: &'a str, capacity: i64) -> Result<Self, RedisError> {
        let tokens = Self::fetch_tokens(ctx, key, capacity)?;

        Ok(Self {
            ctx: ctx,
            key: key,
            capacity: capacity,
            tokens: tokens
        })
    }

    pub fn pour(&mut self, tokens: i64) -> Result<i64, RedisError> {
        if tokens > self.tokens {
            Err(RedisError::Str("ERR bucket is overflown"))
        } else {
            self.tokens -= tokens;
            self.ctx.call("PSETEX", &[self.key, &MAX_TTL.to_string(), &self.tokens.to_string()])?;
            Ok(self.tokens)
        }
    }

    fn fetch_tokens(ctx: &Context, key: &str, capacity: i64) -> Result<i64, RedisError> {
        // Starting with Redis 2.8 the return value of PTTL in case of error changed:
        //     - The command returns -2 if the key does not exist.
        //     - The command returns -1 if the key exists but has no associated expire.
        let current_ttl = match ctx.call("PTTL", &[key])? {
            RedisValue::Integer(ttl) => max(MIN_TTL, ttl),
            _ => MIN_TTL
        };
        let delta = max(MIN_TTL, MAX_TTL - current_ttl) as f64;
        let refilled_tokens = (0.001 * delta * capacity as f64) as i64;
        let remaining_tokens = match ctx.call("GET", &[key])? {
            RedisValue::SimpleString(tokens) => max(MIN_TOKENS, parse_integer(&tokens)?),
            _ => MIN_TOKENS,
        };

        Ok(min(capacity, remaining_tokens + refilled_tokens))
    }
}
