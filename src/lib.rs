#[macro_use] extern crate redis_module;

use redis_module::{Context, RedisError, RedisResult, RedisValue, parse_integer};
use std::cmp::{min, max};

const MAX_TTL: i64 = 1000;
const MIN_TTL: i64 = 0;
const MIN_TOKENS: i64 = 0;
const INSUFFICIENT_TOKENS: i64 = -1;

fn redis_command(ctx: &Context, args: Vec<String>) -> RedisResult {
    if args.len() != 4 {
        return Err(RedisError::WrongArity);
    }

    ctx.auto_memory();

    let bucket = &args[1];
    let capacity = parse_integer(&args[2])?;
    let tokens = parse_integer(&args[3])?;

    // Starting with Redis 2.8 the return value of PTTL in case of error changed:
    //     - The command returns -2 if the key does not exist.
    //     - The command returns -1 if the key exists but has no associated expire.
    let current_ttl = match ctx.call("PTTL", &[bucket])? {
        RedisValue::Integer(ttl) => max(MIN_TTL, ttl),
        _ => MIN_TTL
    };
    let delta = max(MIN_TTL, MAX_TTL - current_ttl) as f64;
    let refilled_tokens = (0.001 * delta * capacity as f64) as i64;

    let mut remaining_tokens = match ctx.call("GET", &[bucket])? {
        RedisValue::Integer(tokens) => tokens,
        RedisValue::SimpleString(tokens) => parse_integer(&tokens)?,
        _ => MIN_TOKENS,
    };
    remaining_tokens = max(MIN_TOKENS, remaining_tokens);
    remaining_tokens = min(capacity, remaining_tokens + refilled_tokens);

    if tokens > remaining_tokens {
        Ok(INSUFFICIENT_TOKENS.into())
    } else {
        remaining_tokens -= tokens;
        ctx.call("PSETEX", &[bucket, &MAX_TTL.to_string(), &remaining_tokens.to_string()])?;
        Ok(remaining_tokens.into())
    }
}

redis_module! {
    name: "SHIELD",
    version: 1,
    data_types: [],
    commands: [
        ["SHIELD.absorb", redis_command, ""],
    ],
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
