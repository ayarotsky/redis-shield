mod bucket;

#[macro_use]
extern crate redis_module;
use bucket::Bucket;
use redis_module::{parse_integer, Context, RedisError, RedisResult};

/*
    SHIELD.absorb user123 30 60 1
        ▲           ▲      ▲  ▲ ▲
        |           |      |  | └─── args[4] apply 1 token (default if omitted)
        |           |      |  └───── args[3] 60 seconds
        |           |      └──────── args[2] 30 tokens
        |           └─────────────── args[1] key "ip-127.0.0.1"
        └─────────────────────────── args[0] command name (provided by redis)
*/
const MIN_ARGS_LEN: usize = 4;
const MAX_ARGS_LEN: usize = 5;
const DEFAULT_TOKENS: i64 = 1;

fn redis_command(ctx: &Context, args: Vec<String>) -> RedisResult {
    if !(MIN_ARGS_LEN..=MAX_ARGS_LEN).contains(&args.len()) {
        return Err(RedisError::WrongArity);
    }

    ctx.auto_memory();

    let capacity = parse_integer(&args[2])?;
    let period = parse_integer(&args[3])?;
    let tokens = match args.len() {
        MAX_ARGS_LEN => parse_integer(&args[4])?,
        _ => DEFAULT_TOKENS,
    };
    let mut bucket = Bucket::new(ctx, &args[1], capacity, period)?;
    let remaining_tokens = bucket.pour(tokens)?;

    Ok(remaining_tokens.into())
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
    fn test_when_bucket_does_not_exist() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_key_new".to_string();

        let _: () = con.del(&bucket_key).unwrap();

        let remaining_tokens: i64 = redis::cmd("SHIELD.absorb")
            .arg(&bucket_key)
            .arg(30)
            .arg(60)
            .query(&mut con)
            .unwrap();
        assert_eq!(remaining_tokens, 29);

        let ttl: i64 = con.pttl(&bucket_key).unwrap();
        assert!(ttl >= 59900 && ttl <= 60000);
    }

    #[test]
    fn test_when_bucket_exist_but_has_no_associated_expire() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_key_no_expire".to_string();

        let _: () = con.del(&bucket_key).unwrap();
        let _: () = con.set(&bucket_key, 2).unwrap();

        let remaining_tokens: i64 = redis::cmd("SHIELD.absorb")
            .arg(&bucket_key)
            .arg(30)
            .arg(60)
            .query(&mut con)
            .unwrap();
        assert_eq!(remaining_tokens, 29);

        let ttl: i64 = con.pttl(&bucket_key).unwrap();
        assert!(ttl >= 59900 && ttl <= 60000);
    }

    #[test]
    fn test_when_multiple_tokens_requested() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_key_multiple_tokens".to_string();

        let _: () = con.del(&bucket_key).unwrap();

        let remaining_tokens: i64 = redis::cmd("SHIELD.absorb")
            .arg(&bucket_key)
            .arg(30)
            .arg(60)
            .arg(25)
            .query(&mut con)
            .unwrap();
        assert_eq!(remaining_tokens, 5);
    }

    #[test]
    #[should_panic(expected = "An error was signalled by the server: bucket is overflown")]
    fn test_when_bucket_is_overflown() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_key_overflown".to_string();

        let _: () = con.del(&bucket_key).unwrap();

        let _: () = redis::cmd("SHIELD.absorb")
            .arg(&bucket_key)
            .arg(30)
            .arg(60)
            .arg(31)
            .query(&mut con)
            .unwrap();
    }

    #[test]
    #[should_panic(expected = "An error was signalled by the server: bucket is overflown")]
    fn test_sequential_requests() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_key_sequential_requests".to_string();
        let tokens = 2;
        let period = 60;

        let _: () = con.del(&bucket_key).unwrap();

        let mut remaining_tokens: i64 = redis::cmd("SHIELD.absorb")
            .arg(&bucket_key)
            .arg(tokens)
            .arg(period)
            .query(&mut con)
            .unwrap();
        assert_eq!(remaining_tokens, 1);

        let mut ttl: i64 = con.pttl(&bucket_key).unwrap();
        assert!(ttl >= 59900 && ttl <= 60000);

        remaining_tokens = redis::cmd("SHIELD.absorb")
            .arg(&bucket_key)
            .arg(tokens)
            .arg(period)
            .query(&mut con)
            .unwrap();
        assert_eq!(remaining_tokens, 0);

        ttl = con.pttl(&bucket_key).unwrap();
        assert!(ttl >= 59900 && ttl <= 60000);

        let _: () = redis::cmd("SHIELD.absorb")
            .arg(&bucket_key)
            .arg(tokens)
            .arg(period)
            .query(&mut con)
            .unwrap();
    }

    #[test]
    fn test_bucket_refills_with_time() {
        let mut con = establish_connection();
        let bucket_key = "redis-shield::test_key_refill".to_string();
        let tokens = 3;
        let period = 6;

        let _: () = con.del(&bucket_key).unwrap();

        let mut remaining_tokens: i64 = redis::cmd("SHIELD.absorb")
            .arg(&bucket_key)
            .arg(tokens)
            .arg(period)
            .query(&mut con)
            .unwrap();
        assert_eq!(remaining_tokens, 2);

        thread::sleep(time::Duration::from_secs(period / 3 + 1));

        remaining_tokens = redis::cmd("SHIELD.absorb")
            .arg(&bucket_key)
            .arg(tokens)
            .arg(period)
            .query(&mut con)
            .unwrap();
        assert_eq!(remaining_tokens, 2);

        remaining_tokens = redis::cmd("SHIELD.absorb")
            .arg(&bucket_key)
            .arg(tokens)
            .arg(period)
            .arg(2)
            .query(&mut con)
            .unwrap();
        assert_eq!(remaining_tokens, 0);

        thread::sleep(time::Duration::from_secs(6));

        remaining_tokens = redis::cmd("SHIELD.absorb")
            .arg(&bucket_key)
            .arg(tokens)
            .arg(period)
            .query(&mut con)
            .unwrap();
        assert_eq!(remaining_tokens, 2);
    }
}
