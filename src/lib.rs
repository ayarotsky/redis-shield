mod bucket;

#[macro_use]
extern crate redis_module;
use bucket::Bucket;
use redis_module::{parse_integer, Context, RedisError, RedisResult};

const MIN_ARGS_LEN: usize = 4;
const MAX_ARGS_LEN: usize = 5;
const DEFAULT_TOKENS: i64 = 1;
const REDIS_COMMAND: &str = "SHIELD.absorb";

/// Entry point to `SHIELD.absorb` redis command.
///
/// * Accepts arguments in the following format:
///       SHIELD.absorb user123 30 60 1
///           ▲           ▲      ▲  ▲ ▲
///           |           |      |  | └─── args[4] apply 1 token (default if omitted)
///           |           |      |  └───── args[3] 60 seconds
///           |           |      └──────── args[2] 30 tokens
///           |           └─────────────── args[1] key "user123"
///           └─────────────────────────── args[0] command name (provided by redis)
///
/// * Parses and validates them
/// * Instantiates a bucket
/// * Attempts to remove requested number of tokens from the bucket
/// * Returns the result of `pour` function.
fn redis_command(ctx: &Context, args: Vec<String>) -> RedisResult {
    if !(MIN_ARGS_LEN..=MAX_ARGS_LEN).contains(&args.len()) {
        return Err(RedisError::WrongArity);
    }

    ctx.auto_memory();

    let capacity = parse_positive_integer("capacity", &args[2])?;
    let period = parse_positive_integer("period", &args[3])?;
    let tokens = match args.len() {
        MAX_ARGS_LEN => parse_positive_integer("tokens", &args[4])?,
        _ => DEFAULT_TOKENS,
    };
    let mut bucket = Bucket::new(ctx, &args[1], capacity, period)?;
    let remaining_tokens = bucket.pour(tokens)?;

    Ok(remaining_tokens.into())
}

fn parse_positive_integer(name: &str, value: &String) -> Result<i64, RedisError> {
    match parse_integer(value) {
        Ok(arg) if arg > 0 => Ok(arg),
        _ => Err(RedisError::String(format!(
            "ERR {} is not positive integer",
            name
        ))),
    }
}

redis_module! {
    name: "SHIELD",
    version: 1,
    data_types: [],
    commands: [
        [REDIS_COMMAND, redis_command, ""],
    ],
}

//////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    extern crate redis;
    use redis::Commands;
    use std::env;
    use std::{thread, time};

    fn establish_connection() -> redis::Connection {
        let redis_url = env::var("REDIS_URL").unwrap();
        let client = redis::Client::open(redis_url).unwrap();
        client.get_connection().unwrap()
    }

    #[test]
    #[should_panic(
        expected = "An error was signalled by the server: wrong number of arguments for 'SHIELD.absorb' command"
    )]
    fn test_wrong_arity() {
        let mut con = establish_connection();

        let _: () = redis::cmd(super::REDIS_COMMAND).query(&mut con).unwrap();
    }

    #[test]
    #[should_panic(
        expected = "An error was signalled by the server: capacity is not positive integer"
    )]
    fn test_when_capacity_is_string() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_key_new";

        let _: () = redis::cmd(super::REDIS_COMMAND)
            .arg(bucket_key)
            .arg("abc")
            .arg(60)
            .query(&mut con)
            .unwrap();
    }

    #[test]
    #[should_panic(
        expected = "An error was signalled by the server: capacity is not positive integer"
    )]
    fn test_when_capacity_is_float() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_key_new";

        let _: () = redis::cmd(super::REDIS_COMMAND)
            .arg(bucket_key)
            .arg(1.2)
            .arg(60)
            .query(&mut con)
            .unwrap();
    }

    #[test]
    #[should_panic(
        expected = "An error was signalled by the server: capacity is not positive integer"
    )]
    fn test_when_capacity_is_zero() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_key_new";

        let _: () = redis::cmd(super::REDIS_COMMAND)
            .arg(bucket_key)
            .arg(0)
            .arg(60)
            .query(&mut con)
            .unwrap();
    }

    #[test]
    #[should_panic(
        expected = "An error was signalled by the server: capacity is not positive integer"
    )]
    fn test_when_capacity_is_negative_integer() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_key_new";

        let _: () = redis::cmd(super::REDIS_COMMAND)
            .arg(bucket_key)
            .arg(-2)
            .arg(60)
            .query(&mut con)
            .unwrap();
    }

    #[test]
    #[should_panic(
        expected = "An error was signalled by the server: period is not positive integer"
    )]
    fn test_when_period_is_string() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_key_new";

        let _: () = redis::cmd(super::REDIS_COMMAND)
            .arg(bucket_key)
            .arg(10)
            .arg("abc")
            .query(&mut con)
            .unwrap();
    }

    #[test]
    #[should_panic(
        expected = "An error was signalled by the server: period is not positive integer"
    )]
    fn test_when_period_is_float() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_key_new";

        let _: () = redis::cmd(super::REDIS_COMMAND)
            .arg(bucket_key)
            .arg(10)
            .arg(6.0)
            .query(&mut con)
            .unwrap();
    }

    #[test]
    #[should_panic(
        expected = "An error was signalled by the server: period is not positive integer"
    )]
    fn test_when_period_is_zero() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_key_new";

        let _: () = redis::cmd(super::REDIS_COMMAND)
            .arg(bucket_key)
            .arg(10)
            .arg(0)
            .query(&mut con)
            .unwrap();
    }

    #[test]
    #[should_panic(
        expected = "An error was signalled by the server: period is not positive integer"
    )]
    fn test_when_period_is_negative_integer() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_key_new";

        let _: () = redis::cmd(super::REDIS_COMMAND)
            .arg(bucket_key)
            .arg(10)
            .arg(-4)
            .query(&mut con)
            .unwrap();
    }

    #[test]
    #[should_panic(
        expected = "An error was signalled by the server: tokens is not positive integer"
    )]
    fn test_when_tokens_is_string() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_key_new";

        let _: () = redis::cmd(super::REDIS_COMMAND)
            .arg(bucket_key)
            .arg(10)
            .arg(60)
            .arg("abc")
            .query(&mut con)
            .unwrap();
    }

    #[test]
    #[should_panic(
        expected = "An error was signalled by the server: tokens is not positive integer"
    )]
    fn test_when_tokens_is_float() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_key_new";

        let _: () = redis::cmd(super::REDIS_COMMAND)
            .arg(bucket_key)
            .arg(10)
            .arg(60)
            .arg(3.1)
            .query(&mut con)
            .unwrap();
    }

    #[test]
    #[should_panic(
        expected = "An error was signalled by the server: tokens is not positive integer"
    )]
    fn test_when_tokens_is_zero() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_key_new";

        let _: () = redis::cmd(super::REDIS_COMMAND)
            .arg(bucket_key)
            .arg(10)
            .arg(60)
            .arg(0)
            .query(&mut con)
            .unwrap();
    }

    #[test]
    #[should_panic(
        expected = "An error was signalled by the server: tokens is not positive integer"
    )]
    fn test_when_tokens_is_negative_integer() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_key_new";

        let _: () = redis::cmd(super::REDIS_COMMAND)
            .arg(bucket_key)
            .arg(10)
            .arg(60)
            .arg(-9)
            .query(&mut con)
            .unwrap();
    }

    #[test]
    fn test_when_bucket_does_not_exist() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_key_new";

        let _: () = con.del(bucket_key).unwrap();

        let remaining_tokens: i64 = redis::cmd(super::REDIS_COMMAND)
            .arg(bucket_key)
            .arg(30)
            .arg(60)
            .query(&mut con)
            .unwrap();
        assert_eq!(remaining_tokens, 29);

        let ttl: i64 = con.pttl(bucket_key).unwrap();
        assert!(ttl >= 59900 && ttl <= 60000);
    }

    #[test]
    fn test_when_bucket_exist_but_has_no_associated_expire() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_key_no_expire";

        let _: () = con.del(bucket_key).unwrap();
        let _: () = con.set(bucket_key, 2).unwrap();

        let remaining_tokens: i64 = redis::cmd(super::REDIS_COMMAND)
            .arg(bucket_key)
            .arg(30)
            .arg(60)
            .query(&mut con)
            .unwrap();
        assert_eq!(remaining_tokens, 29);

        let ttl: i64 = con.pttl(bucket_key).unwrap();
        assert!(ttl >= 59900 && ttl <= 60000);
    }

    #[test]
    fn test_when_multiple_tokens_requested() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_key_multiple_tokens";

        let _: () = con.del(bucket_key).unwrap();

        let remaining_tokens: i64 = redis::cmd(super::REDIS_COMMAND)
            .arg(bucket_key)
            .arg(30)
            .arg(60)
            .arg(25)
            .query(&mut con)
            .unwrap();
        assert_eq!(remaining_tokens, 5);
    }

    #[test]
    fn test_when_bucket_is_overflown() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_key_overflown";

        let _: () = con.del(bucket_key).unwrap();

        let remaining_tokens: i64 = redis::cmd(super::REDIS_COMMAND)
            .arg(bucket_key)
            .arg(30)
            .arg(60)
            .arg(31)
            .query(&mut con)
            .unwrap();
        assert_eq!(remaining_tokens, -1);
    }

    #[test]
    fn test_sequential_requests() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_key_sequential_requests";
        let tokens = 2;
        let period = 60;

        let _: () = con.del(bucket_key).unwrap();

        let mut remaining_tokens: i64 = redis::cmd(super::REDIS_COMMAND)
            .arg(bucket_key)
            .arg(tokens)
            .arg(period)
            .query(&mut con)
            .unwrap();
        assert_eq!(remaining_tokens, 1);

        let mut ttl: i64 = con.pttl(bucket_key).unwrap();
        assert!(ttl >= 59900 && ttl <= 60000);

        remaining_tokens = redis::cmd(super::REDIS_COMMAND)
            .arg(bucket_key)
            .arg(tokens)
            .arg(period)
            .query(&mut con)
            .unwrap();
        assert_eq!(remaining_tokens, 0);

        ttl = con.pttl(bucket_key).unwrap();
        assert!(ttl >= 59900 && ttl <= 60000);

        remaining_tokens = redis::cmd(super::REDIS_COMMAND)
            .arg(bucket_key)
            .arg(tokens)
            .arg(period)
            .query(&mut con)
            .unwrap();
        assert_eq!(remaining_tokens, -1);

        ttl = con.pttl(bucket_key).unwrap();
        assert!(ttl >= 59900 && ttl <= 60000);
    }

    #[test]
    fn test_bucket_refills_with_time() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_key_refill";
        let tokens = 3;
        let period = 6;

        let _: () = con.del(bucket_key).unwrap();

        let mut remaining_tokens: i64 = redis::cmd(super::REDIS_COMMAND)
            .arg(bucket_key)
            .arg(tokens)
            .arg(period)
            .query(&mut con)
            .unwrap();
        assert_eq!(remaining_tokens, 2);

        thread::sleep(time::Duration::from_secs(period / 3 + 1));

        remaining_tokens = redis::cmd(super::REDIS_COMMAND)
            .arg(bucket_key)
            .arg(tokens)
            .arg(period)
            .query(&mut con)
            .unwrap();
        assert_eq!(remaining_tokens, 2);

        remaining_tokens = redis::cmd(super::REDIS_COMMAND)
            .arg(bucket_key)
            .arg(tokens)
            .arg(period)
            .arg(2)
            .query(&mut con)
            .unwrap();
        assert_eq!(remaining_tokens, 0);

        thread::sleep(time::Duration::from_secs(6));

        remaining_tokens = redis::cmd(super::REDIS_COMMAND)
            .arg(bucket_key)
            .arg(tokens)
            .arg(period)
            .query(&mut con)
            .unwrap();
        assert_eq!(remaining_tokens, 2);
    }
}
