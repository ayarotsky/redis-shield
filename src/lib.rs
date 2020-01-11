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
    let mut bucket = Bucket::new(ctx, &args[1], capacity)?;
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
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
